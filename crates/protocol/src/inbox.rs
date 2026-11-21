//! Pull request, check and notification data.

use crate::action::Action;
use crate::bump::Bump;
use crate::capabilities::Capabilities;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A pull request, identified by repository (`owner/name`) and number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrRef {
    pub repo: String,
    pub number: u64,
}

impl fmt::Display for PrRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.repo, self.number)
    }
}

/// Why GitHub notified the user about a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotifyReason {
    ReviewRequested,
    Author,
    Subscribed,
    Mention,
    Assign,
    Comment,
    StateChange,
    CiActivity,
    Other(String),
}

/// Outcome of one check run or commit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckState {
    Success,
    Failure,
    Pending,
    Neutral,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRun {
    pub name: String,
    pub state: CheckState,
    pub details_url: Option<String>,
    pub workflow_run_id: Option<u64>,
}

/// Aggregated CI state of a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CiSummary {
    Green,
    Failing,
    Pending,
    /// No checks at all.
    None,
}

impl CiSummary {
    /// No checks gives `None`; any failure (or cancellation, treated conservatively) gives
    /// `Failing`; otherwise any pending check gives `Pending`; otherwise `Green` (neutral and
    /// skipped checks do not count against it).
    pub fn from_checks(checks: &[CheckRun]) -> Self {
        if checks.is_empty() {
            return Self::None;
        }
        let any = |wanted: &[CheckState]| checks.iter().any(|check| wanted.contains(&check.state));
        if any(&[CheckState::Failure, CheckState::Cancelled]) {
            Self::Failing
        } else if any(&[CheckState::Pending]) {
            Self::Pending
        } else {
            Self::Green
        }
    }
}

/// Mirrors GitHub's `mergeStateStatus`. The kebab-case names are also the `state` values of
/// `rules.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeState {
    Clean,
    Blocked,
    Unstable,
    Dirty,
    Behind,
    Draft,
    HasHooks,
    Unknown,
}

/// Mirrors GitHub's `reviewDecision`; `None` when no review is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
    None,
}

/// How a pull request is merged. Serialized with the variant names (`"Squash"`): it is part
/// of the [`Action`] JSON in `audit.jsonl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

/// The notification thread behind a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationInfo {
    pub thread_id: String,
    pub reason: NotifyReason,
    pub unread: bool,
    pub updated_at: String,
}

/// Lifecycle state of a pull request. Only open ones are bucketed; merged and closed ones are
/// listed (with a notification) so they can be marked read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

/// One pull request of the inbox with everything needed to bucket and act on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrItem {
    pub pr: PrRef,
    /// GraphQL node id; the mutations address the pull request with it.
    pub node_id: String,
    pub title: String,
    pub url: String,
    pub author: String,
    pub is_bot: bool,
    pub draft: bool,
    /// Head commit; mutations pass it as `expectedHeadOid`.
    pub head_oid: String,
    pub merge_state: MergeState,
    pub review_decision: ReviewDecision,
    pub checks: Vec<CheckRun>,
    pub labels: Vec<String>,
    pub changed_files: u32,
    pub created_at: String,
    pub updated_at: String,
    pub allowed_merge_methods: Vec<MergeMethod>,
    /// Open, merged or closed; the last two only appear with a notification.
    pub state: PrState,
    /// What the viewer may do here (permissions, repository settings).
    pub caps: Capabilities,
    /// The viewer's latest review is an approval of the current head commit.
    pub viewer_approved: bool,
    /// The dependency bump the pull request performs, when it is one.
    pub bump: Option<Bump>,
    /// Set when another pull request supersedes this one.
    pub duplicate_of: Option<PrRef>,
    // Why the pull request is in the inbox (any combination):
    /// The notification thread, when there is one.
    pub notification: Option<NotificationInfo>,
    /// The viewer's own pull request.
    pub is_mine: bool,
    /// Found by the `review-requested:@me` search.
    pub review_requested: bool,
    /// Covered by the repository scope of `settings.toml` (whether or not it has a notification).
    pub in_scope: bool,
}

impl PrItem {
    /// Whether the PR belongs in the inbox for a reason other than its notification: an open PR
    /// that is the viewer's own, awaits the viewer's review or is covered by the repository scope.
    pub fn has_other_source(&self) -> bool {
        self.state == PrState::Open && (self.is_mine || self.review_requested || self.in_scope)
    }

    /// Found only through the repository scope of `settings.toml`: no notification, not
    /// review-requested from the viewer and not the viewer's own.
    pub fn via_scope_only(&self) -> bool {
        self.notification.is_none() && !self.is_mine && !self.review_requested
    }

    /// A draft, or GitHub reports the `DRAFT` merge state.
    pub fn is_draft(&self) -> bool {
        self.draft || self.merge_state == MergeState::Draft
    }

    /// The aggregated state of the checks.
    pub fn ci(&self) -> CiSummary {
        CiSummary::from_checks(&self.checks)
    }

    /// Forgets the notification (done or unsubscribed). Returns whether the PR stays in the inbox.
    pub fn drop_notification(&mut self) -> bool {
        self.notification = None;
        self.has_other_source()
    }

    /// Sets the lifecycle state after a merge or close. Merged and closed PRs are only listed
    /// because of their notification: returns whether the PR stays in the inbox.
    pub fn settle(&mut self, state: PrState) -> bool {
        self.state = state;
        self.notification.is_some()
    }

    /// An open, clean bot pull request without checks, notification or other source, for tests
    /// (feature `test-support`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn sample(repo: &str, number: u64, title: &str) -> Self {
        Self {
            pr: PrRef {
                repo: repo.to_owned(),
                number,
            },
            node_id: "PR_node".to_owned(),
            title: title.to_owned(),
            url: format!("https://github.com/{repo}/pull/{number}"),
            author: "renovate[bot]".to_owned(),
            is_bot: true,
            draft: false,
            head_oid: "abc".to_owned(),
            merge_state: MergeState::Clean,
            review_decision: ReviewDecision::None,
            checks: Vec::new(),
            labels: Vec::new(),
            changed_files: 1,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
            allowed_merge_methods: Vec::new(),
            state: PrState::Open,
            caps: Capabilities::default(),
            viewer_approved: false,
            bump: None,
            duplicate_of: None,
            notification: None,
            is_mine: false,
            review_requested: false,
            in_scope: false,
        }
    }
}

/// Inbox section a pull request is shown in (assignment logic lives elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Bucket {
    Mine,
    Ready,
    NeedsApproval,
    Failing,
    Pending,
    Draft,
}

/// The state the backend sends to the UI after a refresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxSnapshot {
    pub items: Vec<PrItem>,
    pub fetched_at: String,
    pub error: Option<String>,
}

impl InboxSnapshot {
    /// Applies a successful action to the PR locally, without waiting for GitHub: mark read
    /// clears `unread`; mark done and unsubscribe drop the notification and remove the PR when
    /// nothing else keeps it in the inbox; merge and close set the state (the PR stays while it
    /// has a notification). Returns whether the snapshot changed.
    pub fn apply_action(&mut self, pr: &PrRef, action: &Action) -> bool {
        let Some(index) = self.items.iter().position(|item| item.pr == *pr) else {
            return false;
        };
        let item = &mut self.items[index];
        let keep = match action {
            Action::MarkRead => {
                return match &mut item.notification {
                    Some(notification) if notification.unread => {
                        notification.unread = false;
                        true
                    }
                    _ => false,
                };
            }
            Action::MarkDone | Action::Unsubscribe => item.drop_notification(),
            Action::Merge(_) | Action::ApproveAndMerge(_) => item.settle(PrState::Merged),
            Action::Close { .. } => item.settle(PrState::Closed),
            _ => return false,
        };
        if !keep {
            self.items.remove(index);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(state: CheckState) -> CheckRun {
        CheckRun {
            name: "ci".to_owned(),
            state,
            details_url: None,
            workflow_run_id: Some(7),
        }
    }

    #[test]
    fn ci_summary() {
        assert_eq!(CiSummary::from_checks(&[]), CiSummary::None);
        let green = [check(CheckState::Success), check(CheckState::Skipped)];
        assert_eq!(CiSummary::from_checks(&green), CiSummary::Green);
        let pending = [check(CheckState::Success), check(CheckState::Pending)];
        assert_eq!(CiSummary::from_checks(&pending), CiSummary::Pending);
        let failing = [check(CheckState::Pending), check(CheckState::Failure)];
        assert_eq!(CiSummary::from_checks(&failing), CiSummary::Failing);
    }

    fn notified(number: u64) -> PrItem {
        let mut item = PrItem::sample("a/b", number, "t");
        item.notification = Some(NotificationInfo {
            thread_id: number.to_string(),
            reason: NotifyReason::Mention,
            unread: true,
            updated_at: String::new(),
        });
        item
    }

    fn snapshot(items: Vec<PrItem>) -> InboxSnapshot {
        InboxSnapshot {
            items,
            fetched_at: String::new(),
            error: None,
        }
    }

    #[test]
    fn done_removes_pr_without_other_source() {
        let mut snap = snapshot(vec![notified(1), notified(2)]);
        assert!(snap.apply_action(&notified(1).pr, &Action::MarkDone));
        assert_eq!(snap.items.len(), 1);
        assert_eq!(snap.items[0].pr.number, 2);
    }

    #[test]
    fn done_keeps_mine_review_requested_and_scope_items() {
        for set in [
            |i: &mut PrItem| i.is_mine = true,
            |i: &mut PrItem| i.review_requested = true,
            |i: &mut PrItem| i.in_scope = true,
        ] {
            let mut item = notified(1);
            set(&mut item);
            let pr = item.pr.clone();
            let mut snap = snapshot(vec![item]);
            assert!(snap.apply_action(&pr, &Action::Unsubscribe));
            assert_eq!(snap.items.len(), 1);
            assert!(snap.items[0].notification.is_none());
        }
        let mut item = notified(1);
        item.in_scope = true;
        assert!(item.drop_notification());
        assert!(item.via_scope_only());
    }

    #[test]
    fn done_removes_merged_and_closed_always() {
        for state in [PrState::Merged, PrState::Closed] {
            let mut item = notified(1);
            item.state = state;
            item.is_mine = true;
            item.in_scope = true;
            let pr = item.pr.clone();
            let mut snap = snapshot(vec![item]);
            snap.apply_action(&pr, &Action::MarkDone);
            assert!(snap.items.is_empty());
        }
    }

    #[test]
    fn merge_and_close_set_the_state_and_keep_notified_items() {
        for (action, state) in [
            (Action::Merge(None), PrState::Merged),
            (Action::ApproveAndMerge(None), PrState::Merged),
            (Action::Close { comment: None }, PrState::Closed),
        ] {
            let mut snap = snapshot(vec![notified(1)]);
            assert!(snap.apply_action(&notified(1).pr, &action));
            assert_eq!(snap.items[0].state, state);
            assert!(snap.items[0].notification.is_some());

            let mut plain = notified(2);
            plain.notification = None;
            let pr = plain.pr.clone();
            let mut snap = snapshot(vec![plain]);
            assert!(snap.apply_action(&pr, &action));
            assert!(snap.items.is_empty());
        }
        let mut snap = snapshot(vec![notified(1)]);
        assert!(!snap.apply_action(&notified(1).pr, &Action::EnableAutoMerge(None)));
        assert_eq!(snap.items[0].state, PrState::Open);
    }

    #[test]
    fn mark_read_only_clears_unread() {
        let mut snap = snapshot(vec![notified(1)]);
        let pr = notified(1).pr;
        assert!(snap.apply_action(&pr, &Action::MarkRead));
        let info = snap.items[0].notification.as_ref().unwrap();
        assert!(!info.unread);
        assert!(!snap.apply_action(&pr, &Action::MarkRead));
        assert!(!snap.apply_action(&pr, &Action::Approve));
        assert!(!snap.apply_action(&notified(9).pr, &Action::MarkDone));
    }

    #[test]
    fn item_roundtrip() {
        let mut item = notified(3);
        item.checks = vec![check(CheckState::Success)];
        item.allowed_merge_methods = vec![MergeMethod::Squash];
        item.notification.as_mut().unwrap().reason = NotifyReason::Other("x".to_owned());
        let snapshot = InboxSnapshot {
            items: vec![item],
            fetched_at: "now".to_owned(),
            error: None,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<InboxSnapshot>(&json).unwrap(),
            snapshot
        );
    }

    fn json<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn wire_enums_are_kebab_case() {
        assert_eq!(json(&MergeState::HasHooks), "\"has-hooks\"");
        assert_eq!(
            json(&ReviewDecision::ChangesRequested),
            "\"changes-requested\""
        );
        assert_eq!(json(&NotifyReason::ReviewRequested), "\"review-requested\"");
        assert_eq!(json(&Bucket::NeedsApproval), "\"needs-approval\"");
        // Part of the action JSON in audit.jsonl: unchanged.
        assert_eq!(json(&MergeMethod::Squash), "\"Squash\"");
    }

    #[test]
    fn scope_only_and_draft_are_derived() {
        let mut item = PrItem::sample("a/b", 1, "t");
        assert!(item.via_scope_only());
        item.review_requested = true;
        assert!(!item.via_scope_only());
        assert!(!notified(2).via_scope_only());
        assert!(!item.is_draft());
        item.merge_state = MergeState::Draft;
        assert!(item.is_draft());
    }
}
