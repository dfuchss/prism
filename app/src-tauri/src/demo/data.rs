//! The demo inbox: fictional organisations (`acme`, `example-org`, `octo-labs`), people
//! (`alice`, `bob`, the viewer `demo-user`) and the two dependency bots, covering every bucket.

use super::VIEWER;
use protocol::{
    AutoMerge, Bump, Capabilities, CheckRun, CheckState, MergeMethod, MergeState, NotificationInfo,
    NotifyReason, PrItem, PrRef, PrState, ReviewDecision, UpdateKind, rfc3339_secs_ago,
};

/// Repositories the viewer can access without an open pull request in the demo.
pub(super) const EXTRA_REPOS: [&str; 5] = [
    "acme/infra",
    "acme/legacy-portal",
    "acme/legacy-billing",
    "example-org/blog",
    "octo-labs/homebrew-tap",
];

/// The pull request whose merge fails (a realistic error in the progress panel).
pub(super) const FAILING_MERGE: (&str, u64) = ("acme/api-gateway", 88);

/// GitHub's merge state and review decision, as far as they matter for the bucket.
#[derive(Clone, Copy)]
enum Shape {
    Clean,
    /// Waits for a required review.
    Blocked,
    /// Conflicts with the base branch.
    Dirty,
    Behind,
    Draft,
    Merged,
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ci {
    Green,
    /// The second and third check of the repository fail.
    Red,
    /// The last check of the repository still runs.
    Running,
    None,
}

/// Why the pull request is in the inbox.
#[derive(Clone, Copy)]
enum Why {
    /// A notification (reason, unread).
    Notified(Reason, bool),
    /// A review request from the viewer, with an unread notification or a read one.
    Review(bool),
    /// The viewer's own pull request, with a notification (unread or not).
    Mine(bool),
    /// Only through the repository scope of `settings.toml`.
    Scope,
}

#[derive(Clone, Copy)]
enum Reason {
    Subscribed,
    CiActivity,
    Mention,
    StateChange,
}

/// `(dependency, from, to, kind)`.
type BumpSpec = (&'static str, Option<&'static str>, &'static str, UpdateKind);

struct Spec {
    repo: &'static str,
    number: u64,
    author: &'static str,
    title: &'static str,
    /// Minutes since the last update.
    minutes: u64,
    shape: Shape,
    ci: Ci,
    bump: Option<BumpSpec>,
    why: Why,
}

const RENOVATE: &str = "renovate[bot]";
const DEPENDABOT: &str = "dependabot[bot]";
const HOUR: u64 = 60;
const DAY: u64 = 24 * HOUR;

#[expect(
    clippy::too_many_arguments,
    reason = "a table row; named fields would triple the data's length"
)]
const fn spec(
    (repo, number): (&'static str, u64),
    author: &'static str,
    title: &'static str,
    minutes: u64,
    shape: Shape,
    ci: Ci,
    bump: Option<BumpSpec>,
    why: Why,
) -> Spec {
    Spec {
        repo,
        number,
        author,
        title,
        minutes,
        shape,
        ci,
        bump,
        why,
    }
}

use Ci::{Green, Red, Running};
use Shape::{Behind, Blocked, Clean, Closed, Dirty, Draft, Merged};
use UpdateKind::{Major, Minor, Patch, Unknown};
use Why::{Mine, Notified, Review, Scope};

/// Mergeable bumps and an approved feature: the Ready bucket.
#[rustfmt::skip]
fn ready() -> Vec<Spec> {
    let sub = |unread| Notified(Reason::Subscribed, unread);
    vec![
        spec(("acme/widget", 412), RENOVATE, "Update rust crate serde to v1.0.219", 12, Clean, Green,
            Some(("serde", Some("1.0.210"), "1.0.219", Patch)), sub(true)),
        spec(FAILING_MERGE, DEPENDABOT, "Bump tokio from 1.41.1 to 1.42.0", 25, Clean, Green,
            Some(("tokio", Some("1.41.1"), "1.42.0", Minor)), sub(true)),
        spec(("example-org/docs", 156), DEPENDABOT, "Bump actions/checkout from 4.1.7 to 4.2.2", 41,
            Clean, Green, Some(("actions/checkout", Some("4.1.7"), "4.2.2", Minor)), sub(true)),
        spec(("acme/api-gateway", 87), RENOVATE, "Update rust crate clap to v4.5.23", 63, Clean,
            Green, Some(("clap", Some("4.5.20"), "4.5.23", Patch)), sub(false)),
        spec(("acme/widget", 409), DEPENDABOT, "Bump serde from 1.0.210 to 1.0.215", 3 * HOUR,
            Clean, Green, Some(("serde", Some("1.0.210"), "1.0.215", Patch)), sub(false)),
        spec(("example-org/website", 312), RENOVATE, "Update dependency eslint to v9.17.0",
            5 * HOUR, Clean, Green, Some(("eslint", Some("9.15.0"), "9.17.0", Minor)), sub(false)),
        spec(("acme/widget", 405), "alice", "Add keyboard shortcuts to the search box", 7 * HOUR,
            Clean, Green, None, Review(false)),
        spec(("acme/design-system", 49), DEPENDABOT, "Bump @storybook/react from 8.4.5 to 8.4.7",
            9 * HOUR, Clean, Green, Some(("@storybook/react", Some("8.4.5"), "8.4.7", Patch)), Scope),
        spec(("acme/design-system", 50), RENOVATE, "Update dependency prettier to v3.4.2", 26 * HOUR,
            Clean, Green, Some(("prettier", Some("3.3.3"), "3.4.2", Minor)), Scope),
    ]
}

/// Review requests, failing and pending pull requests.
#[rustfmt::skip]
fn waiting() -> Vec<Spec> {
    vec![
        spec(("acme/api-gateway", 91), "alice", "Rate-limit unauthenticated requests", 35, Blocked,
            Green, None, Review(true)),
        spec(("octo-labs/cli", 228), "bob", "Add --json output to the list command", 2 * HOUR,
            Blocked, Running, None, Review(true)),
        spec(("example-org/docs", 160), RENOVATE, "Update dependency typescript to v5.7.2",
            4 * HOUR, Blocked, Green, Some(("typescript", Some("5.6.3"), "5.7.2", Minor)),
            Notified(Reason::Subscribed, false)),
        spec(("acme/widget", 398), RENOVATE, "Update dependency react-router-dom to v7.0.2", 18,
            Clean, Red, Some(("react-router-dom", Some("6.28.0"), "7.0.2", Major)),
            Notified(Reason::CiActivity, true)),
        spec(("octo-labs/sdk-python", 64), DEPENDABOT, "Bump pydantic from 2.9.2 to 2.10.3",
            3 * HOUR, Clean, Red, Some(("pydantic", Some("2.9.2"), "2.10.3", Minor)),
            Notified(Reason::CiActivity, false)),
        spec(("acme/api-gateway", 85), "bob", "Refactor the token cache", 22 * HOUR, Dirty, Green,
            None, Notified(Reason::Mention, true)),
        spec(("acme/design-system", 52), RENOVATE, "Update dependency vite to v6.0.3", 8, Clean,
            Running, Some(("vite", Some("5.4.11"), "6.0.3", Major)), Notified(Reason::Subscribed, true)),
        spec(("octo-labs/cli", 233), DEPENDABOT, "Bump golang.org/x/net from 0.31.0 to 0.32.0",
            6 * HOUR, Behind, Green, Some(("golang.org/x/net", Some("0.31.0"), "0.32.0", Minor)),
            Notified(Reason::Subscribed, false)),
        spec(("example-org/website", 311), RENOVATE, "Update actions/setup-node digest to 39370e3",
            50, Clean, Running, Some(("actions/setup-node", None, "39370e3", Unknown)),
            Notified(Reason::Subscribed, false)),
        spec(("acme/api-gateway", 84), DEPENDABOT, "Bump hyper from 1.5.0 to 1.5.1", 30 * HOUR,
            Clean, Running, Some(("hyper", Some("1.5.0"), "1.5.1", Patch)), Scope),
    ]
}

/// Drafts, the viewer's own pull requests and merged or closed ones with a notification.
#[rustfmt::skip]
fn other() -> Vec<Spec> {
    let changed = |unread| Notified(Reason::StateChange, unread);
    vec![
        spec(("acme/widget", 401), "alice", "New onboarding flow", 2 * DAY, Draft, Green, None,
            Notified(Reason::Subscribed, false)),
        spec(("octo-labs/sdk-python", 61), "bob", "Async client prototype", 3 * DAY, Draft,
            Ci::None, None, Notified(Reason::Mention, true)),
        spec(("acme/widget", 410), VIEWER, "Fix flaky retry test in the HTTP client", 90, Clean,
            Green, None, Mine(false)),
        spec(("example-org/docs", 158), VIEWER, "Document the rules file syntax", 20 * HOUR,
            Blocked, Running, None, Mine(true)),
        spec(("acme/widget", 395), DEPENDABOT, "Bump regex from 1.11.0 to 1.11.1", 2 * HOUR,
            Merged, Green, Some(("regex", Some("1.11.0"), "1.11.1", Patch)), changed(true)),
        spec(("octo-labs/cli", 225), RENOVATE, "Update module github.com/spf13/cobra to v1.8.1",
            DAY, Closed, Green, Some(("github.com/spf13/cobra", Some("1.8.0"), "1.8.1", Patch)),
            changed(false)),
        spec(("example-org/website", 305), "alice", "Fix broken links in the footer", 5 * HOUR,
            Merged, Green, None, changed(false)),
    ]
}

/// The whole demo world, newest first.
pub(super) fn items() -> Vec<PrItem> {
    let mut items: Vec<PrItem> = [ready(), waiting(), other()]
        .into_iter()
        .flatten()
        .map(|spec| item(&spec))
        .collect();
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    items
}

/// The check names of a repository (its CI workflow and the code scanning).
fn check_names(repo: &str) -> &'static [&'static str] {
    match repo {
        "acme/widget" | "acme/api-gateway" => &[
            "build",
            "test (ubuntu-latest)",
            "test (macos-latest)",
            "clippy",
            "CodeQL",
        ],
        "example-org/docs" => &["build-docs", "link-check", "markdownlint"],
        "octo-labs/cli" => &["build", "test (go 1.23)", "golangci-lint"],
        "octo-labs/sdk-python" => &["ruff", "pytest (3.12)", "pytest (3.13)", "mypy"],
        _ => &["build", "unit-tests", "e2e (chromium)", "lint"],
    }
}

/// The workflow run of a pull request's checks.
pub(super) fn run_id(pr: &PrRef) -> u64 {
    12_000_000_000 + hash(pr) % 900_000_000
}

fn checks(pr: &PrRef, ci: Ci) -> Vec<CheckRun> {
    if ci == Ci::None {
        return Vec::new();
    }
    let names = check_names(&pr.repo);
    let run = run_id(pr);
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let state = match ci {
                Ci::Red if index == 1 || index == 2 => CheckState::Failure,
                Ci::Running if index + 1 == names.len() => CheckState::Pending,
                _ => CheckState::Success,
            };
            CheckRun {
                name: (*name).to_owned(),
                state,
                details_url: Some(format!(
                    "https://github.com/{}/actions/runs/{run}/job/{}",
                    pr.repo,
                    run * 10 + u64::try_from(index).unwrap_or_default()
                )),
                workflow_run_id: Some(run),
            }
        })
        .collect()
}

/// What the viewer may do in a repository: auto-merge is off in `example-org/docs`, and
/// `octo-labs/sdk-python` is read-only.
fn caps(repo: &str) -> Capabilities {
    match repo {
        "example-org/docs" => Capabilities {
            auto_merge: AutoMerge::Unavailable(
                "Auto-merge is disabled in this repository's settings".to_owned(),
            ),
            ..Capabilities::default()
        },
        "octo-labs/sdk-python" => Capabilities {
            can_merge: false,
            can_update_branch: false,
            auto_merge: AutoMerge::Unavailable(
                "You need write access to this repository".to_owned(),
            ),
            ..Capabilities::default()
        },
        _ => Capabilities::default(),
    }
}

fn merge_methods(repo: &str) -> Vec<MergeMethod> {
    match repo.split_once('/').map(|(owner, _)| owner) {
        Some("acme") => vec![MergeMethod::Squash],
        Some("example-org") => vec![MergeMethod::Squash, MergeMethod::Merge],
        _ => vec![MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase],
    }
}

/// `(merge state, review decision, lifecycle state)` of a shape; the review decision of a
/// clean pull request is an approval for people and none (not required) for bots.
fn states(shape: Shape, bot: bool) -> (MergeState, ReviewDecision, PrState) {
    let clean_review = if bot {
        ReviewDecision::None
    } else {
        ReviewDecision::Approved
    };
    match shape {
        Shape::Clean => (MergeState::Clean, clean_review, PrState::Open),
        Shape::Blocked => (
            MergeState::Blocked,
            ReviewDecision::ReviewRequired,
            PrState::Open,
        ),
        Shape::Dirty => (MergeState::Dirty, ReviewDecision::None, PrState::Open),
        Shape::Behind => (MergeState::Behind, ReviewDecision::None, PrState::Open),
        Shape::Draft => (MergeState::Draft, ReviewDecision::None, PrState::Open),
        Shape::Merged => (MergeState::Clean, clean_review, PrState::Merged),
        Shape::Closed => (MergeState::Clean, ReviewDecision::None, PrState::Closed),
    }
}

fn notification(pr: &PrRef, why: Why, updated_at: &str) -> Option<NotificationInfo> {
    let (reason, unread) = match why {
        Why::Notified(reason, unread) => (
            match reason {
                Reason::Subscribed => NotifyReason::Subscribed,
                Reason::CiActivity => NotifyReason::CiActivity,
                Reason::Mention => NotifyReason::Mention,
                Reason::StateChange => NotifyReason::StateChange,
            },
            unread,
        ),
        Why::Review(unread) => (NotifyReason::ReviewRequested, unread),
        Why::Mine(unread) => (NotifyReason::Author, unread),
        Why::Scope => return None,
    };
    Some(NotificationInfo {
        thread_id: (10_000_000_000 + hash(pr) % 9_000_000_000).to_string(),
        reason,
        unread,
        updated_at: updated_at.to_owned(),
    })
}

fn item(spec: &Spec) -> PrItem {
    let pr = PrRef {
        repo: spec.repo.to_owned(),
        number: spec.number,
    };
    let bot = spec.author.ends_with("[bot]");
    let (merge_state, review_decision, state) = states(spec.shape, bot);
    let updated_at = rfc3339_secs_ago(spec.minutes * 60);
    let created_minutes = spec.minutes + 60 * (hash(&pr) % 72);
    PrItem {
        node_id: format!("PR_kwDOdemo{}", hash(&pr) % 1_000_000),
        title: spec.title.to_owned(),
        url: format!("https://github.com/{}/pull/{}", pr.repo, pr.number),
        author: spec.author.to_owned(),
        is_bot: bot,
        draft: matches!(spec.shape, Shape::Draft),
        head_oid: format!(
            "{:016x}{:016x}{:08x}",
            hash(&pr),
            hash(&pr).rotate_left(17),
            pr.number
        ),
        merge_state,
        review_decision,
        checks: checks(&pr, spec.ci),
        labels: if bot {
            vec!["dependencies".to_owned()]
        } else {
            Vec::new()
        },
        changed_files: if bot {
            2
        } else {
            3 + u32::try_from(pr.number % 14).unwrap_or(0)
        },
        created_at: rfc3339_secs_ago(created_minutes * 60),
        allowed_merge_methods: merge_methods(spec.repo),
        state,
        caps: caps(spec.repo),
        viewer_approved: false,
        bump: spec.bump.map(|(dependency, from, to, kind)| Bump {
            dependency: dependency.to_owned(),
            from: from.map(str::to_owned),
            to: to.to_owned(),
            kind,
            directory: None,
        }),
        duplicate_of: None,
        notification: notification(&pr, spec.why, &updated_at),
        is_mine: matches!(spec.why, Why::Mine(_)),
        review_requested: matches!(spec.why, Why::Review(_)),
        in_scope: false,
        updated_at,
        pr,
    }
}

/// A stable pseudo-random number per pull request (FNV-1a), for ids and commit hashes.
fn hash(pr: &PrRef) -> u64 {
    pr.repo
        .bytes()
        .chain(pr.number.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}
