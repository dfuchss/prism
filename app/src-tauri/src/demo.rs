//! Demo mode (`PRISM_DEMO=1`), a development option for screenshots and trying the UI: a
//! fictional inbox instead of GitHub, actions simulated locally, and a throwaway config
//! directory. No token, `gh` or network is used, and the real config directory is never read
//! or written.
//!
//! Environment (only read when `PRISM_DEMO=1`):
//! - `PRISM_CONFIG_DIR`: the config directory (default: `prism-demo` in the temp directory);
//!   seeded with example rules and a scope when it has no `rules.toml` / `settings.toml`.
//! - `PRISM_DEMO_VIEW`: the screen the UI starts on (`inbox`, `bulk`, `planned`, `rules`,
//!   `settings`, `audit`, `about`); `bulk` also starts a merge batch that stops after two jobs.
//! - `PRISM_DEMO_THEME`: `light` (default) or `dark`.
//! - `PRISM_DEMO_WINDOW_POSITION`: `x,y` of the window (logical pixels); its size is 1280×800.

mod data;

use crate::state::lock;
use prism_github::Details;
use protocol::{
    Action, ActionResult, AutoMerge, CheckState, FailedJob, InboxSnapshot, MergeState, PrItem,
    PrRef, PrState, ReviewDecision, now_rfc3339,
};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// The demo viewer's login.
pub(crate) const VIEWER: &str = "demo-user";

/// How long a simulated action takes (long enough to see the progress panel).
const ACTION_DELAY: Duration = Duration::from_millis(300);
/// In the `bulk` view, jobs after this many wait (practically) forever: the panel stays
/// mid-batch for the screenshot.
const BULK_HOLD_AFTER: usize = 2;
const BULK_HOLD: Duration = Duration::from_hours(24);

/// Window size in demo mode (logical pixels).
const WINDOW_SIZE: (f64, f64) = (1280.0, 800.0);

const SETTINGS: &str = r#"# Demo settings (PRISM_DEMO=1): the open pull requests of every acme repository are added
# to the inbox ("via repo"), except the legacy ones.

[scope]
include = ["acme/*"]
exclude = ["acme/legacy-*"]
"#;

/// The example rules of `examples/rules` that the Planned tab shows, all as suggestions.
const RULES: [&str; 5] = [
    include_str!("../../../examples/rules/merge-green-patch-minor.toml"),
    include_str!("../../../examples/rules/approve-green-bot-prs.toml"),
    include_str!("../../../examples/rules/close-duplicates.toml"),
    include_str!("../../../examples/rules/rebase-stale-bot-prs.toml"),
    include_str!("../../../examples/rules/mark-merged-read.toml"),
];

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Whether demo mode is on (`PRISM_DEMO=1`).
pub(crate) fn enabled() -> bool {
    env("PRISM_DEMO").as_deref() == Some("1")
}

/// The demo config directory (`PRISM_CONFIG_DIR` or `prism-demo` in the temp directory),
/// seeded with the demo rules and settings where they are missing.
pub(crate) fn config_dir() -> std::io::Result<PathBuf> {
    let dir = env("PRISM_CONFIG_DIR")
        .map_or_else(|| std::env::temp_dir().join("prism-demo"), PathBuf::from);
    seed(&dir)?;
    Ok(dir)
}

fn seed(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, text) in [
        ("rules.toml", rules_toml()),
        ("settings.toml", SETTINGS.to_owned()),
    ] {
        let path = dir.join(name);
        if !path.exists() {
            std::fs::write(path, text)?;
        }
    }
    Ok(())
}

/// The example rules joined, with `auto = true` turned into suggestions (an auto rule would
/// act right after the start and change the demo inbox before it is seen).
fn rules_toml() -> String {
    let mut text = String::from(
        "# Demo rules (PRISM_DEMO=1): the examples of examples/rules, all as suggestions.\n\n",
    );
    for rule in RULES {
        for line in rule.lines() {
            text.push_str(if line == "auto = true" {
                "auto = false"
            } else {
                line
            });
            text.push('\n');
        }
        text.push('\n');
    }
    text
}

/// Sizes and places the main window, pins it on top and sets the appearance.
pub(crate) fn setup_window(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{LogicalPosition, LogicalSize, Manager, Theme};
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    window.set_size(LogicalSize::new(WINDOW_SIZE.0, WINDOW_SIZE.1))?;
    let position = env("PRISM_DEMO_WINDOW_POSITION").and_then(|text| {
        let (x, y) = text.split_once(',')?;
        Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))
    });
    if let Some((x, y)) = position {
        window.set_position(LogicalPosition::new(x, y))?;
    }
    window.set_always_on_top(true)?;
    let dark = env("PRISM_DEMO_THEME").as_deref() == Some("dark");
    window.set_theme(Some(if dark { Theme::Dark } else { Theme::Light }))
}

/// The demo inbox and its simulated GitHub.
pub(crate) struct DemoSource {
    /// Every demo pull request, whatever brings it into the inbox.
    world: Mutex<Vec<PrItem>>,
    view: Option<String>,
    started: AtomicUsize,
}

impl DemoSource {
    pub(crate) fn new() -> Self {
        Self::with_view(env("PRISM_DEMO_VIEW"))
    }

    fn with_view(view: Option<String>) -> Self {
        Self {
            world: Mutex::new(data::items()),
            view,
            started: AtomicUsize::new(0),
        }
    }

    pub(crate) fn view(&self) -> Option<&str> {
        self.view.as_deref()
    }

    /// What GitHub would list: notified pull requests, open ones that are the viewer's own or
    /// request the viewer's review, and the open ones of `scope`.
    pub(crate) fn inbox(&self, scope: &[PrRef]) -> InboxSnapshot {
        let scope: HashSet<&PrRef> = scope.iter().collect();
        let items = lock(&self.world)
            .iter()
            .filter_map(|item| {
                let mut item = item.clone();
                item.in_scope = item.state == PrState::Open && scope.contains(&item.pr);
                (item.notification.is_some() || item.has_other_source()).then_some(item)
            })
            .collect();
        InboxSnapshot {
            items,
            fetched_at: now_rfc3339(),
            error: None,
        }
    }

    /// Fresh details without the inbox sources, as `Client::pr_details` returns them.
    pub(crate) fn pr_details(&self, prs: &[PrRef]) -> Details {
        let world = lock(&self.world);
        let items = prs
            .iter()
            .filter_map(|pr| world.iter().find(|item| item.pr == *pr))
            .map(|item| PrItem {
                notification: None,
                is_mine: false,
                review_requested: false,
                in_scope: false,
                ..item.clone()
            })
            .collect();
        Details {
            items,
            warnings: Vec::new(),
        }
    }

    pub(crate) fn failed_jobs(&self, repo: &str, run_id: u64) -> Vec<FailedJob> {
        lock(&self.world)
            .iter()
            .filter(|item| item.pr.repo == repo)
            .flat_map(|item| &item.checks)
            .filter(|check| check.workflow_run_id == Some(run_id))
            .filter(|check| check.state == CheckState::Failure)
            .map(|check| FailedJob {
                name: check.name.clone(),
                html_url: check.details_url.clone().unwrap_or_default(),
            })
            .collect()
    }

    pub(crate) fn accessible_repos(&self) -> Vec<String> {
        let world = lock(&self.world);
        let repos: BTreeSet<String> = world
            .iter()
            .map(|item| item.pr.repo.clone())
            .chain(data::EXTRA_REPOS.map(str::to_owned))
            .collect();
        repos.into_iter().collect()
    }

    pub(crate) fn open_pr_refs(&self, repos: &[String]) -> Vec<PrRef> {
        lock(&self.world)
            .iter()
            .filter(|item| item.state == PrState::Open && repos.contains(&item.pr.repo))
            .map(|item| item.pr.clone())
            .collect()
    }

    /// Simulates the action after a short delay and updates the demo world like GitHub would.
    pub(crate) async fn perform(&self, item: &PrItem, action: &Action) -> ActionResult {
        let started = self.started.fetch_add(1, Ordering::SeqCst);
        let hold = self.view.as_deref() == Some("bulk") && started >= BULK_HOLD_AFTER;
        tokio::time::sleep(if hold { BULK_HOLD } else { ACTION_DELAY }).await;
        let outcome = self.apply(&item.pr, action);
        ActionResult {
            pr: item.pr.clone(),
            action: action.clone(),
            ok: outcome.is_ok(),
            message: outcome.unwrap_or_else(|error| error),
        }
    }

    fn apply(&self, pr: &PrRef, action: &Action) -> Result<String, String> {
        let merging = matches!(action, Action::Merge(_) | Action::ApproveAndMerge(_));
        if merging && (pr.repo.as_str(), pr.number) == data::FAILING_MERGE {
            return Err(
                "GraphQL error: Base branch was modified. Review and try the merge again."
                    .to_owned(),
            );
        }
        let mut world = lock(&self.world);
        let item = world
            .iter_mut()
            .find(|item| item.pr == *pr)
            .ok_or_else(|| format!("{pr} not found"))?;
        apply_to(item, action)
    }
}

/// What a successful action changes on GitHub, and its result message.
fn apply_to(item: &mut PrItem, action: &Action) -> Result<String, String> {
    let message = match action {
        Action::Approve => approve(item),
        Action::Merge(_) => settle(item, PrState::Merged),
        Action::ApproveAndMerge(_) => {
            approve(item);
            settle(item, PrState::Merged)
        }
        Action::EnableAutoMerge(_) => {
            item.caps.auto_merge = AutoMerge::AlreadyEnabled;
            "auto-merge enabled"
        }
        Action::Close { .. } => settle(item, PrState::Closed),
        Action::Comment(_) => "commented",
        Action::RerunFailed => rerun(item)?,
        Action::Rebase => {
            item.merge_state = MergeState::Clean;
            if item.is_bot {
                "asked the bot to rebase"
            } else {
                "branch updated"
            }
        }
        Action::MarkRead | Action::MarkDone | Action::Unsubscribe => notify(item, action)?,
    };
    Ok(message.to_owned())
}

fn approve(item: &mut PrItem) -> &'static str {
    if item.viewer_approved {
        return "already approved";
    }
    item.viewer_approved = true;
    item.review_decision = ReviewDecision::Approved;
    if item.merge_state == MergeState::Blocked {
        item.merge_state = MergeState::Clean;
    }
    "approved"
}

fn settle(item: &mut PrItem, state: PrState) -> &'static str {
    item.state = state;
    if state == PrState::Merged {
        "merged"
    } else {
        "closed"
    }
}

fn rerun(item: &mut PrItem) -> Result<&'static str, String> {
    let mut failing = item
        .checks
        .iter_mut()
        .filter(|check| check.state == CheckState::Failure)
        .peekable();
    if failing.peek().is_none() {
        return Err("no failing workflow run to rerun".to_owned());
    }
    failing.for_each(|check| check.state = CheckState::Pending);
    Ok("re-running failed jobs of 1 workflow run(s)")
}

fn notify(item: &mut PrItem, action: &Action) -> Result<&'static str, String> {
    let notification = item
        .notification
        .as_mut()
        .ok_or_else(|| "no notification thread for this PR".to_owned())?;
    Ok(match action {
        Action::MarkRead => {
            notification.unread = false;
            "marked read"
        }
        Action::MarkDone => {
            item.notification = None;
            "marked done"
        }
        _ => {
            item.notification = None;
            "unsubscribed"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use prism_github::mark_duplicates;
    use protocol::{Bucket, UpdateKind, bucket};
    use std::collections::HashSet;

    const OWNERS: [&str; 3] = ["acme", "example-org", "octo-labs"];
    const PEOPLE: [&str; 5] = ["renovate[bot]", "dependabot[bot]", "alice", "bob", VIEWER];

    fn scope_refs(demo: &DemoSource) -> Vec<PrRef> {
        let repos: Vec<String> = demo
            .accessible_repos()
            .into_iter()
            .filter(|repo| repo.starts_with("acme/") && !repo.starts_with("acme/legacy-"))
            .collect();
        demo.open_pr_refs(&repos)
    }

    fn inbox(demo: &DemoSource) -> Vec<PrItem> {
        let mut items = demo.inbox(&scope_refs(demo)).items;
        mark_duplicates(&mut items);
        items
    }

    #[test]
    fn only_fictional_owners_and_people() {
        let demo = DemoSource::with_view(None);
        for repo in demo.accessible_repos() {
            let owner = repo.split_once('/').map(|(owner, _)| owner).unwrap();
            assert!(OWNERS.contains(&owner), "{repo}");
        }
        for item in inbox(&demo) {
            assert!(PEOPLE.contains(&item.author.as_str()), "{}", item.author);
            assert!(
                item.url
                    .starts_with(&format!("https://github.com/{}/", item.pr.repo))
            );
            for check in &item.checks {
                let url = check.details_url.as_deref().unwrap();
                assert!(url.starts_with(&format!("https://github.com/{}/", item.pr.repo)));
            }
        }
    }

    #[test]
    fn every_bucket_and_kind_of_item_is_present() {
        let items = inbox(&DemoSource::with_view(None));
        assert!((20..=30).contains(&items.len()), "{}", items.len());
        let open: Vec<&PrItem> = items.iter().filter(|i| i.state == PrState::Open).collect();
        let buckets: HashSet<Bucket> = open.iter().map(|item| bucket(item)).collect();
        for wanted in [
            Bucket::Ready,
            Bucket::NeedsApproval,
            Bucket::Failing,
            Bucket::Pending,
            Bucket::Draft,
            Bucket::Mine,
        ] {
            assert!(buckets.contains(&wanted), "{wanted:?}");
        }
        let kinds: Vec<UpdateKind> = items
            .iter()
            .filter_map(|i| i.bump.as_ref())
            .map(|b| b.kind)
            .collect();
        for kind in [
            UpdateKind::Patch,
            UpdateKind::Minor,
            UpdateKind::Major,
            UpdateKind::Unknown,
        ] {
            assert!(kinds.contains(&kind), "{kind:?}");
        }
        assert!(items.iter().any(|i| i.duplicate_of.is_some()));
        assert!(items.iter().any(|i| i.state == PrState::Merged));
        assert!(items.iter().any(|i| i.state == PrState::Closed));
        assert!(
            items
                .iter()
                .all(|i| i.state == PrState::Open || i.notification.is_some())
        );
        assert!(
            items
                .iter()
                .any(|i| i.notification.as_ref().is_some_and(|n| n.unread))
        );
        assert!(open.iter().any(|i| i.via_scope_only() && i.in_scope));
        assert!(
            items
                .iter()
                .any(|i| matches!(i.caps.auto_merge, AutoMerge::Unavailable(_)))
        );
    }

    #[test]
    fn failing_checks_have_failed_jobs() {
        let demo = DemoSource::with_view(None);
        let failing: Vec<PrItem> = inbox(&demo)
            .into_iter()
            .filter(|item| item.checks.iter().any(|c| c.state == CheckState::Failure))
            .collect();
        assert!(!failing.is_empty());
        for item in failing {
            let run = item.checks[0].workflow_run_id.unwrap();
            assert!(
                !demo.failed_jobs(&item.pr.repo, run).is_empty(),
                "{}",
                item.pr
            );
        }
    }

    #[test]
    fn actions_change_the_world_like_github() {
        let demo = DemoSource::with_view(None);
        let ready = inbox(&demo)
            .into_iter()
            .find(|item| bucket(item) == Bucket::Ready && item.notification.is_some())
            .unwrap();
        let merged = demo.apply(&ready.pr, &Action::Merge(None));
        assert_eq!(merged.as_deref(), Ok("merged"));
        let after = inbox(&demo).into_iter().find(|i| i.pr == ready.pr).unwrap();
        assert_eq!(after.state, PrState::Merged);
        demo.apply(&ready.pr, &Action::MarkDone).unwrap();
        assert!(inbox(&demo).iter().all(|i| i.pr != ready.pr));
        let (repo, number) = data::FAILING_MERGE;
        let failing = PrRef {
            repo: repo.to_owned(),
            number,
        };
        assert!(
            demo.apply(&failing, &Action::ApproveAndMerge(None))
                .is_err()
        );
    }

    #[test]
    fn seeded_rules_are_valid_suggestions_and_settings_parse() {
        let (rules, warnings) = config::parse_rules(&rules_toml()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(rules.rules.len(), 5);
        assert!(rules.rules.iter().all(|rule| !rule.auto));
        let (settings, _) = config::parse_settings(SETTINGS).unwrap();
        assert_eq!(settings.scope.include, ["acme/*"]);
        let planned = prism_rules::evaluate(&rules.rules, &inbox(&DemoSource::with_view(None)));
        assert!(planned.len() >= 5, "{}", planned.len());
    }

    #[test]
    fn seeding_keeps_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("rules.toml"), "# mine\n").unwrap();
        seed(dir.path()).unwrap();
        let rules = std::fs::read_to_string(dir.path().join("rules.toml")).unwrap();
        assert_eq!(rules, "# mine\n");
        assert!(dir.path().join("settings.toml").exists());
    }
}
