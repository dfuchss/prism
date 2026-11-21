//! Tauri commands. Arguments are camelCase on the JS side; errors are plain messages. Only
//! commands that can fail return a `Result`. File I/O runs off the main thread (async commands
//! with [`config::blocking`]).

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects `State` and deserializes arguments by value; commands cannot take references"
)]

use crate::config;
use crate::events;
use crate::exec::{Job, execute};
use crate::refresh;
use crate::scope;
use crate::settings::Settings;
use crate::state::{AppState, Loaded, lock};
use prism_rules::evaluate;
use protocol::{
    ActTarget, Action, ActionResult, AppInfo, AuditEntry, CheckState, FailedJob, InboxSnapshot,
    PlannedAction, PrRef, ProgressSource, ScopePreview, Viewer,
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

type CommandResult<T> = Result<T, String>;

/// The cached inbox, `None` before the first refresh finished.
#[tauri::command]
pub(crate) fn get_inbox(state: State<'_, AppState>) -> Option<InboxSnapshot> {
    lock(&state.snapshot).clone()
}

#[tauri::command]
pub(crate) async fn refresh(app: AppHandle) -> InboxSnapshot {
    refresh::refresh(&app).await
}

/// Runs `action` on every target PR (sequentially), audits it, then refreshes in the
/// background. Approvals and merges are refused when a PR has moved on from the head the user
/// saw.
#[tauri::command]
pub(crate) async fn act(
    app: AppHandle,
    targets: Vec<ActTarget>,
    action: Action,
) -> Vec<ActionResult> {
    let jobs = targets
        .into_iter()
        .map(|target| Job::manual(target, action.clone()))
        .collect();
    run_batch(app, jobs, ProgressSource::Manual).await
}

#[tauri::command]
pub(crate) async fn apply_planned(
    app: AppHandle,
    planned: Vec<PlannedAction>,
) -> Vec<ActionResult> {
    let jobs = planned.into_iter().map(Job::planned).collect();
    run_batch(app, jobs, ProgressSource::Planned).await
}

/// Runs a manual or planned batch. The UI hears about its progress and the locally updated
/// snapshot; afterwards the auto rules run on it, then the real refresh.
async fn run_batch(app: AppHandle, jobs: Vec<Job>, source: ProgressSource) -> Vec<ActionResult> {
    let state = app.state::<AppState>();
    let results = execute(&state, jobs, None, source, &events::batch(&app)).await;
    if !results.is_empty() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            refresh::after_batch(&app).await;
        });
    }
    results
}

/// Refreshes in the background; the UI shows the Refresh spinner meanwhile.
fn refresh_later(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let _running = events::refreshing(&app);
        refresh::refresh(&app).await;
    });
}

/// Failed jobs of every distinct failing workflow run of the PR.
#[tauri::command]
pub(crate) async fn failed_jobs(
    state: State<'_, AppState>,
    pr: PrRef,
) -> CommandResult<Vec<FailedJob>> {
    let runs = failing_runs(&state, &pr)?;
    let mut jobs = Vec::new();
    for run in runs {
        let found = state
            .client
            .failed_jobs(&pr.repo, run)
            .await
            .map_err(|error| error.to_string())?;
        jobs.extend(found);
    }
    Ok(jobs)
}

fn failing_runs(state: &AppState, pr: &PrRef) -> CommandResult<Vec<u64>> {
    let snapshot = lock(&state.snapshot);
    let item = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.items.iter().find(|item| item.pr == *pr))
        .ok_or_else(|| format!("{pr} is not in the inbox"))?;
    let mut runs: Vec<u64> = Vec::new();
    for check in item
        .checks
        .iter()
        .filter(|c| c.state == CheckState::Failure)
    {
        if let Some(run) = check.workflow_run_id.filter(|run| !runs.contains(run)) {
            runs.push(run);
        }
    }
    Ok(runs)
}

#[tauri::command]
pub(crate) async fn get_rules(app: AppHandle) -> CommandResult<String> {
    let dir = app.state::<AppState>().config_dir.clone();
    config::blocking(move || config::ensure_rules(&dir))
        .await
        .map_err(|error| error.to_string())
}

/// Validates and saves `rules.toml`; returns warnings. Invalid TOML or unknown actions are an
/// error and nothing is written. The UI is told right away (the Planned tab re-evaluates), then
/// the auto rules run once.
#[tauri::command]
pub(crate) async fn save_rules(app: AppHandle, toml: String) -> CommandResult<Vec<String>> {
    let (rules, warnings) = config::parse_rules(&toml)?;
    let dir = app.state::<AppState>().config_dir.clone();
    config::blocking(move || config::write_rules(&dir, &toml))
        .await
        .map_err(|error| format!("could not save rules.toml: {error}"))?;
    *lock(&app.state::<AppState>().rules) = Loaded::ok(rules);
    events::inbox_cached(&app);
    tauri::async_runtime::spawn(async move {
        refresh::auto_once(&app).await;
    });
    Ok(warnings)
}

#[tauri::command]
pub(crate) async fn get_settings(app: AppHandle) -> CommandResult<String> {
    let dir = app.state::<AppState>().config_dir.clone();
    config::blocking(move || config::ensure_settings(&dir))
        .await
        .map_err(|error| error.to_string())
}

/// Validates and saves `settings.toml`; returns warnings. Invalid TOML is an error and nothing
/// is written, and neither is a `[gh] path` that is no executable file. The cached inbox is
/// rescoped and announced right away, then the repository list is fetched anew, the token
/// forgotten and the inbox refreshed (which brings the newly covered repositories).
#[tauri::command]
pub(crate) async fn save_settings(app: AppHandle, toml: String) -> CommandResult<Vec<String>> {
    let (settings, warnings) = config::parse_settings(&toml)?;
    let dir = app.state::<AppState>().config_dir.clone();
    let settings = config::blocking(move || {
        config::check_gh_path(&settings)?;
        config::write_settings(&dir, &toml)
            .map_err(|error| format!("could not save settings.toml: {error}"))?;
        Ok::<_, String>(settings)
    })
    .await?;
    apply_settings(&app.state::<AppState>(), settings);
    events::inbox_cached(&app);
    refresh_later(app);
    Ok(warnings)
}

/// Makes saved settings current: the new `gh` path (the cached token is dropped, so the next
/// request uses it), the scope (a fetch still running for the old one is not stored), a
/// forgotten repository list and the cached inbox rescoped.
fn apply_settings(state: &AppState, settings: Settings) {
    state.client.set_gh_path(&settings.gh.path);
    let scope = settings.scope.clone();
    *lock(&state.settings) = Loaded::ok(settings);
    // Counted before the snapshot is locked; see `refresh::store`.
    state.settings_generation.fetch_add(1, Ordering::SeqCst);
    scope::invalidate(state);
    if let Some(snapshot) = lock(&state.snapshot).as_mut() {
        scope::rescope(snapshot, &scope);
    }
}

/// The accessible repositories the scope of `toml` (not saved) covers and excludes.
#[tauri::command]
pub(crate) async fn scope_preview(
    state: State<'_, AppState>,
    toml: String,
) -> CommandResult<ScopePreview> {
    let (settings, _) = config::parse_settings(&toml)?;
    scope::preview(&state, &settings.scope).await
}

/// What all rules (auto or not) would do to the cached inbox.
#[tauri::command]
pub(crate) fn planned_actions(state: State<'_, AppState>) -> Vec<PlannedAction> {
    let rules = lock(&state.rules).value.rules.clone();
    let items = lock(&state.snapshot)
        .as_ref()
        .map_or_default(|snapshot| snapshot.items.clone());
    evaluate(&rules, &items)
}

/// The newest `limit` audit entries, newest first.
#[tauri::command]
pub(crate) async fn audit_log(app: AppHandle, limit: usize) -> Vec<AuditEntry> {
    let dir = app.state::<AppState>().config_dir.clone();
    config::blocking(move || config::read_audit(&dir, limit)).await
}

/// Version, platform, config directory and GitHub connection status, for the About tab.
#[tauri::command]
pub(crate) async fn app_info(app: AppHandle) -> AppInfo {
    let state = app.state::<AppState>();
    let viewer = match state.client.viewer_login().await {
        Ok(login) => Viewer::SignedIn(login),
        Err(error) => Viewer::Unavailable(error.to_string()),
    };
    AppInfo {
        version: app.package_info().version.to_string(),
        tauri_version: tauri::VERSION.to_owned(),
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        config_dir: state.config_dir.display().to_string(),
        auth_source: state.client.auth_source(),
        viewer,
    }
}

/// The screen the UI starts on in demo mode (`PRISM_DEMO_VIEW`); `None` normally.
#[tauri::command]
pub(crate) fn demo_view(state: State<'_, AppState>) -> Option<String> {
    state.client.demo_view()
}

/// Opens a GitHub page in the browser; other URLs are refused.
#[tauri::command]
pub(crate) fn open_url(app: AppHandle, url: String) -> CommandResult<()> {
    if !is_github_url(&url) {
        return Err("only https://github.com/ URLs can be opened".to_owned());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

fn is_github_url(url: &str) -> bool {
    url.starts_with("https://github.com/") && !url.chars().any(|c| c.is_control() || c == ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_settings_rescope_and_invalidate_running_fetches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.toml"), "[scope\n").unwrap();
        let state = AppState::load(dir.path().to_owned()).unwrap();
        let mut scoped = protocol::PrItem::sample("a/old", 1, "t");
        scoped.in_scope = true;
        let mut mine = protocol::PrItem::sample("a/old", 2, "t");
        mine.is_mine = true;
        *lock(&state.snapshot) = Some(InboxSnapshot {
            items: vec![scoped, mine],
            fetched_at: String::new(),
            error: None,
        });
        let (settings, _) = config::parse_settings("[scope]\ninclude = [\"a/new\"]\n").unwrap();
        apply_settings(&state, settings);
        assert_eq!(state.settings_generation.load(Ordering::SeqCst), 1);
        let current = lock(&state.settings);
        assert!(current.error.is_none(), "the load error is gone");
        assert_eq!(current.value.scope.include, ["a/new"]);
        let snapshot = lock(&state.snapshot);
        let numbers: Vec<u64> = snapshot
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.pr.number)
            .collect();
        assert_eq!(numbers, [2]);
    }

    #[test]
    fn only_github_https_urls() {
        assert!(is_github_url("https://github.com/acme/widget/pull/1"));
        assert!(!is_github_url("http://github.com/acme"));
        assert!(!is_github_url("https://github.com.evil.example/x"));
        assert!(!is_github_url("https://evil.example/https://github.com/"));
        assert!(!is_github_url("file:///etc/passwd"));
        assert!(!is_github_url("https://github.com/a b"));
    }
}
