//! Display texts and CSS classes of protocol values. Everything except the clock is pure.

use protocol::{
    Action, ActionResult, CheckState, CiSummary, MergeState, NotifyReason, PrItem, ProgressSource,
    ReviewDecision, UpdateKind,
};

pub(crate) fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Seconds between an RFC 3339 timestamp and `now`; `None` when it does not parse.
pub(crate) fn secs_since(iso: &str, now: f64) -> Option<f64> {
    let then = js_sys::Date::parse(iso);
    (!then.is_nan()).then(|| ((now - then) / 1000.0).max(0.0))
}

/// `now`, `5m`, `3h`, `2d`, `4mo`.
pub(crate) fn age_of(secs: f64) -> String {
    match secs {
        s if s < 60.0 => "now".to_owned(),
        s if s < 3600.0 => format!("{:.0}m", (s / 60.0).floor()),
        s if s < 86_400.0 => format!("{:.0}h", (s / 3600.0).floor()),
        s if s < 2_592_000.0 => format!("{:.0}d", (s / 86_400.0).floor()),
        s => format!("{:.0}mo", (s / 2_592_000.0).floor()),
    }
}

/// [`age_of`] an RFC 3339 timestamp; empty when it does not parse.
pub(crate) fn age(iso: &str, now: f64) -> String {
    secs_since(iso, now).map_or_else(String::new, age_of)
}

/// The top bar's "updated 5m ago" for a fetch `secs` ago.
pub(crate) fn updated(secs: f64) -> String {
    match age_of(secs).as_str() {
        "now" => "updated just now".to_owned(),
        age => format!("updated {age} ago"),
    }
}

pub(crate) fn kind_label(kind: UpdateKind) -> &'static str {
    match kind {
        UpdateKind::Patch => "patch",
        UpdateKind::Minor => "minor",
        UpdateKind::Major => "major",
        UpdateKind::Unknown => "?",
    }
}

pub(crate) fn kind_class(kind: UpdateKind) -> &'static str {
    match kind {
        UpdateKind::Patch => "kind-patch",
        UpdateKind::Minor => "kind-minor",
        UpdateKind::Major => "kind-major",
        UpdateKind::Unknown => "kind-unknown",
    }
}

pub(crate) fn merge_state_label(state: MergeState) -> &'static str {
    match state {
        MergeState::Clean => "clean",
        MergeState::Blocked => "blocked",
        MergeState::Unstable => "unstable (non-required checks fail)",
        MergeState::Dirty => "conflicts",
        MergeState::Behind => "behind the base branch",
        MergeState::Draft => "draft",
        MergeState::HasHooks => "clean (with hooks)",
        MergeState::Unknown => "unknown",
    }
}

pub(crate) fn review_label(review: ReviewDecision) -> &'static str {
    match review {
        ReviewDecision::Approved => "approved",
        ReviewDecision::ChangesRequested => "changes requested",
        ReviewDecision::ReviewRequired => "review required",
        ReviewDecision::None => "not required",
    }
}

pub(crate) fn check_label(state: CheckState) -> &'static str {
    match state {
        CheckState::Success => "success",
        CheckState::Failure => "failure",
        CheckState::Cancelled => "cancelled",
        CheckState::Pending => "pending",
        CheckState::Neutral => "neutral",
        CheckState::Skipped => "skipped",
    }
}

/// Dot colour of a single check.
pub(crate) fn check_class(state: CheckState) -> &'static str {
    match state {
        CheckState::Success => "ci-green",
        CheckState::Failure | CheckState::Cancelled => "ci-red",
        CheckState::Pending => "ci-yellow",
        CheckState::Neutral | CheckState::Skipped => "ci-grey",
    }
}

/// Dot colour of the aggregated checks.
pub(crate) fn ci_class(ci: CiSummary) -> &'static str {
    match ci {
        CiSummary::Green => "ci-green",
        CiSummary::Failing => "ci-red",
        CiSummary::Pending => "ci-yellow",
        CiSummary::None => "ci-grey",
    }
}

pub(crate) fn reason_label(reason: &NotifyReason) -> String {
    match reason {
        NotifyReason::ReviewRequested => "review requested".to_owned(),
        NotifyReason::Author => "author".to_owned(),
        NotifyReason::Subscribed => "subscribed".to_owned(),
        NotifyReason::Mention => "mention".to_owned(),
        NotifyReason::Assign => "assigned".to_owned(),
        NotifyReason::Comment => "comment".to_owned(),
        NotifyReason::StateChange => "state change".to_owned(),
        NotifyReason::CiActivity => "CI".to_owned(),
        NotifyReason::Other(other) => other.replace('_', " "),
    }
}

/// The table's "Why" column: the notification reason, else "mine" or "via repo".
pub(crate) fn why_label(item: &PrItem) -> String {
    match &item.notification {
        Some(notification) => reason_label(&notification.reason),
        None if item.is_mine => "mine".to_owned(),
        None if item.via_scope_only() => "via repo".to_owned(),
        None => String::new(),
    }
}

/// Short human text of an action, for dialogs, toasts and the audit log.
pub(crate) fn action_label(action: &Action) -> String {
    match action {
        Action::Approve => "approve".to_owned(),
        Action::Merge(_) => "merge".to_owned(),
        Action::ApproveAndMerge(_) => "approve + merge".to_owned(),
        Action::EnableAutoMerge(_) => "auto-merge".to_owned(),
        Action::Close { comment: None } => "close".to_owned(),
        Action::Close { comment: Some(_) } => "close with comment".to_owned(),
        Action::Comment(_) => "comment".to_owned(),
        Action::MarkRead => "mark read".to_owned(),
        Action::MarkDone => "mark done".to_owned(),
        Action::Unsubscribe => "unsubscribe".to_owned(),
        Action::RerunFailed => "rerun failed".to_owned(),
        Action::Rebase => "rebase".to_owned(),
    }
}

/// One line of a result list: `merge owner/repo#1: merged`.
pub(crate) fn result_text(result: &ActionResult) -> String {
    format!(
        "{} {}: {}",
        action_label(&result.action),
        result.pr,
        result.message
    )
}

/// `1 pull request`, `3 pull requests`.
pub(crate) fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// Heading of a running batch: "Merging", "Approving", ... or "Auto rules".
pub(crate) fn progress_verb(action: &Action, source: ProgressSource) -> &'static str {
    if source == ProgressSource::Auto {
        return "Auto rules";
    }
    match action {
        Action::Approve => "Approving",
        Action::Merge(_) => "Merging",
        Action::ApproveAndMerge(_) => "Approving + merging",
        Action::EnableAutoMerge(_) => "Enabling auto-merge for",
        Action::Close { .. } => "Closing",
        Action::Comment(_) => "Commenting on",
        Action::MarkRead => "Marking read",
        Action::MarkDone => "Marking done",
        Action::Unsubscribe => "Unsubscribing from",
        Action::RerunFailed => "Rerunning failed jobs of",
        Action::Rebase => "Rebasing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{NotificationInfo, PrRef};

    #[test]
    fn ages_round_down_per_unit() {
        assert_eq!(age_of(0.0), "now");
        assert_eq!(age_of(59.9), "now");
        assert_eq!(age_of(60.0), "1m");
        assert_eq!(age_of(119.0), "1m");
        assert_eq!(age_of(3599.0), "59m");
        assert_eq!(age_of(3600.0), "1h");
        assert_eq!(age_of(86_400.0 * 2.5), "2d");
        assert_eq!(age_of(2_592_000.0 * 4.0), "4mo");
    }

    #[test]
    fn updated_text_uses_the_age() {
        assert_eq!(updated(5.0), "updated just now");
        assert_eq!(updated(5400.0), "updated 1h ago");
    }

    #[test]
    fn kind_classes_are_css_safe() {
        for kind in [
            UpdateKind::Patch,
            UpdateKind::Minor,
            UpdateKind::Major,
            UpdateKind::Unknown,
        ] {
            let class = kind_class(kind);
            assert!(class.starts_with("kind-"));
            assert!(class.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        }
        assert_eq!(kind_label(UpdateKind::Unknown), "?");
    }

    #[test]
    fn labels_are_human_text() {
        assert_eq!(
            review_label(ReviewDecision::ReviewRequired),
            "review required"
        );
        assert_eq!(merge_state_label(MergeState::Dirty), "conflicts");
        assert_eq!(check_label(CheckState::Cancelled), "cancelled");
        assert_eq!(check_class(CheckState::Cancelled), "ci-red");
        assert_eq!(ci_class(CiSummary::None), "ci-grey");
        assert_eq!(
            reason_label(&NotifyReason::Other("team_mention".to_owned())),
            "team mention"
        );
    }

    #[test]
    fn why_prefers_the_notification() {
        let mut item = PrItem::sample("a/b", 1, "t");
        assert_eq!(why_label(&item), "via repo");
        item.is_mine = true;
        assert_eq!(why_label(&item), "mine");
        item.notification = Some(NotificationInfo {
            thread_id: "1".to_owned(),
            reason: NotifyReason::Mention,
            unread: true,
            updated_at: String::new(),
        });
        assert_eq!(why_label(&item), "mention");
        item.notification = None;
        item.is_mine = false;
        item.review_requested = true;
        assert_eq!(why_label(&item), "");
    }

    #[test]
    fn result_and_count_texts() {
        let result = ActionResult {
            pr: PrRef {
                repo: "a/b".to_owned(),
                number: 7,
            },
            action: Action::Close {
                comment: Some("x".to_owned()),
            },
            ok: true,
            message: "closed".to_owned(),
        };
        assert_eq!(result_text(&result), "close with comment a/b#7: closed");
        assert_eq!(count(1, "pull request"), "1 pull request");
        assert_eq!(count(2, "action"), "2 actions");
        assert_eq!(
            progress_verb(&Action::Merge(None), ProgressSource::Auto),
            "Auto rules"
        );
    }
}
