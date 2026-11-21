//! Fetching the inbox, the auto pass and the poller.

use crate::events;
use crate::exec::{Job, execute};
use crate::scope;
use crate::state::{AppState, lock};
use prism_github::{Error, mark_duplicates};
use prism_rules::{Rule, evaluate};
use protocol::{InboxSnapshot, ProgressSource, now_rfc3339};
use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const MIN_POLL_SECS: u64 = 60;

/// Seconds between two polls: what GitHub asks for, but at least a minute.
fn poll_secs(requested: Option<u64>) -> u64 {
    requested.unwrap_or(MIN_POLL_SECS).max(MIN_POLL_SECS)
}

/// Whether an error means the cached items are worthless (no or bad credentials).
fn is_auth_error(error: &Error) -> bool {
    matches!(
        error,
        Error::TokenUnavailable | Error::GhNotStarted(_) | Error::Unauthorized
    )
}

/// Fetches the inbox, stores it, tells the UI. Never fails: errors (and an invalid
/// `rules.toml` or `settings.toml`) end up in `InboxSnapshot.error`. The flag says whether the
/// fetch worked; warnings about single unreadable PRs do not count as failure. Only runs under
/// `refresh_lock`. When the settings changed during the fetch, its result is dropped and the
/// cached (already rescoped) snapshot returned as not fetched: the refresh the change started
/// brings the new scope.
async fn fetch(app: &AppHandle) -> (InboxSnapshot, bool) {
    let state = app.state::<AppState>();
    let generation = state.settings_generation.load(Ordering::SeqCst);
    let (extra, scope_error) = scope::scope_refs(&state).await;
    let fetched = state.client.inbox(&extra).await;
    let (snapshot, ok) = complete(&state, fetched, scope_error, Instant::now());
    if !store(&state, &snapshot, generation) {
        let cached = lock(&state.snapshot).clone().unwrap_or(snapshot);
        return (cached, false);
    }
    events::inbox_updated(app, &snapshot);
    (snapshot, ok)
}

/// Turns a fetch result into the snapshot to show: the local overrides and duplicate marks
/// applied (or, on failure, the items to keep), excluded repositories dropped and every error
/// in `error`. The flag says whether the fetch worked.
fn complete(
    state: &AppState,
    fetched: Result<InboxSnapshot, Error>,
    scope_error: Option<String>,
    now: Instant,
) -> (InboxSnapshot, bool) {
    let (mut snapshot, ok) = match fetched {
        Ok(mut snapshot) => {
            lock(&state.overrides).apply(&mut snapshot.items, now);
            mark_duplicates(&mut snapshot.items);
            (snapshot, true)
        }
        Err(error) => (failed_snapshot(state, &error), false),
    };
    let config_error = join_errors(
        lock(&state.rules).error.clone(),
        lock(&state.settings).error.clone(),
    );
    // `exclude` hides repositories from every source.
    scope::drop_excluded(&mut snapshot.items, &lock(&state.settings).value.scope);
    snapshot.error = join_errors(join_errors(config_error, scope_error), snapshot.error);
    (snapshot, ok)
}

/// Stores `snapshot` unless the settings changed since `generation` (checked under the
/// snapshot lock, which the settings change takes after counting up). Whether it was stored.
fn store(state: &AppState, snapshot: &InboxSnapshot, generation: u64) -> bool {
    let mut cached = lock(&state.snapshot);
    if state.settings_generation.load(Ordering::SeqCst) != generation {
        return false;
    }
    *cached = Some(snapshot.clone());
    true
}

fn join_errors(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}; {second}")),
        (first, second) => first.or(second),
    }
}

/// Authentication problems empty the list; other errors keep the previous items.
fn failed_snapshot(state: &AppState, error: &Error) -> InboxSnapshot {
    let previous = lock(&state.snapshot).clone();
    let items = match previous {
        Some(old) if !is_auth_error(error) => old.items,
        _ => Vec::new(),
    };
    InboxSnapshot {
        items,
        fetched_at: now_rfc3339(),
        error: Some(error.to_string()),
    }
}

/// Runs auto passes one at a time. A pass requested while one runs is not dropped: the running
/// one repeats once it ends.
#[derive(Default)]
pub(crate) struct PassGate {
    /// `(running, again)`.
    flags: Mutex<(bool, bool)>,
}

impl PassGate {
    /// Claims the gate; `false` when a pass runs (it then runs again after it ends).
    fn try_start(&self) -> bool {
        let mut flags = lock(&self.flags);
        if flags.0 {
            flags.1 = true;
            false
        } else {
            flags.0 = true;
            true
        }
    }

    /// Called when a pass ended: `true` when another was requested meanwhile (the gate stays
    /// claimed), otherwise releases it.
    fn again(&self) -> bool {
        let mut flags = lock(&self.flags);
        if flags.1 {
            flags.1 = false;
            true
        } else {
            flags.0 = false;
            false
        }
    }

    /// Releases the gate unconditionally (a pass ended abnormally).
    fn release(&self) {
        *lock(&self.flags) = (false, false);
    }
}

/// A full refresh: fetch, then the auto rules; when they acted, fetch again.
pub(crate) async fn refresh(app: &AppHandle) -> InboxSnapshot {
    let state = app.state::<AppState>();
    let _serial = state.refresh_lock.lock().await;
    let (snapshot, fetched) = fetch(app).await;
    if fetched && run_passes(&state.auto_gate, || run_auto(app)).await > 0 {
        return fetch(app).await.0;
    }
    snapshot
}

/// Releases the gate when a pass ends abnormally (a panic).
struct Claim<'a> {
    gate: &'a PassGate,
    armed: bool,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.gate.release();
        }
    }
}

/// Runs `pass` unless one runs already; then that one repeats after it ends. Returns the
/// number of actions this call's passes did.
async fn run_passes<F, Fut>(gate: &PassGate, mut pass: F) -> usize
where
    F: FnMut() -> Fut,
    Fut: Future<Output = usize>,
{
    if !gate.try_start() {
        return 0;
    }
    let mut claim = Claim { gate, armed: true };
    let mut total = 0;
    loop {
        total += pass().await;
        if !gate.again() {
            claim.armed = false;
            return total;
        }
    }
}

/// After a manual or planned batch: the auto rules act on the locally updated snapshot, then
/// the real refresh runs.
pub(crate) async fn after_batch(app: &AppHandle) {
    let state = app.state::<AppState>();
    run_passes(&state.auto_gate, || run_auto(app)).await;
    refresh(app).await;
}

/// One auto pass (e.g. after `rules.toml` was saved); when it acted, the inbox is fetched anew.
pub(crate) async fn auto_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    if run_passes(&state.auto_gate, || run_auto(app)).await > 0 {
        refresh(app).await;
    }
}

/// Runs the `auto = true` rules on the cached inbox, re-checking fresh details before every
/// action. Returns how many actions succeeded. Callers go through [`run_passes`].
async fn run_auto(app: &AppHandle) -> usize {
    let state = app.state::<AppState>();
    let rules: Vec<Rule> = lock(&state.rules)
        .value
        .rules
        .iter()
        .filter(|rule| rule.auto)
        .cloned()
        .collect();
    let items = lock(&state.snapshot)
        .as_ref()
        .map_or_default(|snapshot| snapshot.items.clone());
    let jobs: Vec<Job> = evaluate(&rules, &items)
        .into_iter()
        .map(Job::planned)
        .collect();
    if jobs.is_empty() {
        return 0;
    }
    let emit = events::batch(app);
    let results = execute(&state, jobs, Some(&rules), ProgressSource::Auto, &emit).await;
    results.iter().filter(|result| result.ok).count()
}

/// Refreshes at startup and then every poll interval, for the life of the app.
pub(crate) fn start_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&app).await;
            let requested = app.state::<AppState>().client.poll_interval_secs();
            tokio::time::sleep(Duration::from_secs(poll_secs(requested))).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_joined() {
        let text = |s: &str| Some(s.to_owned());
        assert_eq!(join_errors(None, None), None);
        assert_eq!(join_errors(text("a"), None), text("a"));
        assert_eq!(join_errors(None, text("b")), text("b"));
        assert_eq!(join_errors(text("a"), text("b")), text("a; b"));
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        tauri::async_runtime::block_on(future)
    }

    #[test]
    fn a_pass_requested_while_one_runs_repeats() {
        let gate = PassGate::default();
        let calls = std::cell::Cell::new(0);
        let total = block_on(run_passes(&gate, || {
            calls.set(calls.get() + 1);
            // A batch ends while the first pass runs.
            let refused = calls.get() == 1 && !gate.try_start();
            assert!(calls.get() > 1 || refused);
            async { 1 }
        }));
        assert_eq!((calls.get(), total), (2, 2));
        assert!(gate.try_start(), "the gate is free afterwards");
    }

    #[test]
    fn gate_defers_a_request_made_while_running() {
        let gate = PassGate::default();
        assert!(gate.try_start());
        assert!(!gate.try_start());
        assert!(gate.again());
        assert!(!gate.again());
        assert!(gate.try_start());
        gate.release();
        assert!(gate.try_start());
    }

    fn state_with(settings: &str) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.toml"), settings).unwrap();
        let state = AppState::load(dir.path().to_owned()).unwrap();
        (dir, state)
    }

    fn snapshot(items: Vec<protocol::PrItem>) -> InboxSnapshot {
        InboxSnapshot {
            items,
            fetched_at: String::new(),
            error: None,
        }
    }

    #[test]
    fn a_fetch_merges_overrides_scope_and_errors() {
        let (_dir, state) = state_with("[scope]\nexclude = [\"x/*\"]\n");
        let now = Instant::now();
        let kept = protocol::PrItem::sample("a/b", 1, "kept");
        let merged = protocol::PrItem::sample("a/b", 2, "merged here");
        let excluded = protocol::PrItem::sample("x/y", 3, "excluded");
        lock(&state.overrides).remember_state(
            &merged.pr,
            protocol::PrState::Merged,
            &merged.updated_at,
            now,
        );
        let fetched = snapshot(vec![kept, merged, excluded]);
        let (result, ok) = complete(&state, Ok(fetched), Some("scope broke".to_owned()), now);
        assert!(ok);
        let numbers: Vec<u64> = result.items.iter().map(|i| i.pr.number).collect();
        assert_eq!(numbers, [1]);
        assert_eq!(result.error.as_deref(), Some("scope broke"));

        // A failed fetch keeps the previous items, an authentication failure empties the list.
        *lock(&state.snapshot) = Some(snapshot(vec![protocol::PrItem::sample("a/b", 4, "old")]));
        let network = Error::Refused("down".to_owned());
        let (kept, ok) = complete(&state, Err(network), None, now);
        assert!(!ok);
        assert_eq!(kept.items.len(), 1);
        assert!(kept.error.unwrap().contains("down"));
        let (emptied, _) = complete(&state, Err(Error::Unauthorized), None, now);
        assert!(emptied.items.is_empty());
    }

    #[test]
    fn config_errors_reach_the_banner() {
        let (_dir, state) = state_with("[scope\n");
        let (result, _) = complete(&state, Ok(snapshot(Vec::new())), None, Instant::now());
        assert!(result.error.unwrap().contains("settings.toml is invalid"));
    }

    #[test]
    fn a_fetch_started_before_a_settings_change_is_not_stored() {
        let (_dir, state) = state_with("");
        let generation = state.settings_generation.load(Ordering::SeqCst);
        let fetched = snapshot(vec![protocol::PrItem::sample("a/b", 1, "t")]);
        state.settings_generation.fetch_add(1, Ordering::SeqCst);
        assert!(!store(&state, &fetched, generation));
        assert!(lock(&state.snapshot).is_none());
        assert!(store(&state, &fetched, generation + 1));
        assert_eq!(lock(&state.snapshot).as_ref(), Some(&fetched));
    }

    #[test]
    fn poll_interval_is_at_least_a_minute() {
        assert_eq!(poll_secs(None), 60);
        assert_eq!(poll_secs(Some(30)), 60);
        assert_eq!(poll_secs(Some(300)), 300);
    }
}
