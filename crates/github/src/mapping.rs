//! GitHub's enum strings (REST and GraphQL) to protocol types and back.

use protocol::{MergeMethod, MergeState, NotifyReason, PrState, ReviewDecision};

/// The `reason` of the REST notifications API (`review_requested`, ...).
pub(crate) fn notify_reason(reason: &str) -> NotifyReason {
    match reason {
        "review_requested" => NotifyReason::ReviewRequested,
        "author" => NotifyReason::Author,
        "subscribed" => NotifyReason::Subscribed,
        "mention" | "team_mention" => NotifyReason::Mention,
        "assign" => NotifyReason::Assign,
        "comment" => NotifyReason::Comment,
        "state_change" => NotifyReason::StateChange,
        "ci_activity" => NotifyReason::CiActivity,
        other => NotifyReason::Other(other.to_owned()),
    }
}

/// `mergeStateStatus` (`CLEAN`, `HAS_HOOKS`, ...); anything else is `Unknown`.
pub(crate) fn merge_state(state: &str) -> MergeState {
    match state {
        "CLEAN" => MergeState::Clean,
        "BLOCKED" => MergeState::Blocked,
        "UNSTABLE" => MergeState::Unstable,
        "DIRTY" => MergeState::Dirty,
        "BEHIND" => MergeState::Behind,
        "DRAFT" => MergeState::Draft,
        "HAS_HOOKS" => MergeState::HasHooks,
        _ => MergeState::Unknown,
    }
}

/// `reviewDecision`; null or unknown is `None`.
pub(crate) fn review_decision(decision: Option<&str>) -> ReviewDecision {
    match decision {
        Some("APPROVED") => ReviewDecision::Approved,
        Some("CHANGES_REQUESTED") => ReviewDecision::ChangesRequested,
        Some("REVIEW_REQUIRED") => ReviewDecision::ReviewRequired,
        _ => ReviewDecision::None,
    }
}

/// The pull request `state` (`OPEN`, `MERGED`, `CLOSED`); anything else is `None`.
pub(crate) fn pr_state(state: &str) -> Option<PrState> {
    match state {
        "OPEN" => Some(PrState::Open),
        "MERGED" => Some(PrState::Merged),
        "CLOSED" => Some(PrState::Closed),
        _ => None,
    }
}

const MERGE_METHODS: [(MergeMethod, &str); 3] = [
    (MergeMethod::Merge, "MERGE"),
    (MergeMethod::Squash, "SQUASH"),
    (MergeMethod::Rebase, "REBASE"),
];

/// The `PullRequestMergeMethod` value of a merge method.
pub(crate) fn merge_method_name(method: MergeMethod) -> &'static str {
    MERGE_METHODS
        .iter()
        .find(|(known, _)| *known == method)
        .map_or("MERGE", |(_, name)| name)
}

/// The merge method of a `PullRequestMergeMethod` value.
pub(crate) fn merge_method(name: &str) -> Option<MergeMethod> {
    MERGE_METHODS
        .iter()
        .find(|(_, known)| *known == name)
        .map(|(method, _)| *method)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_states_and_methods() {
        assert_eq!(
            notify_reason("review_requested"),
            NotifyReason::ReviewRequested
        );
        assert_eq!(notify_reason("team_mention"), NotifyReason::Mention);
        assert_eq!(
            notify_reason("security_alert"),
            NotifyReason::Other("security_alert".to_owned())
        );
        assert_eq!(merge_state("HAS_HOOKS"), MergeState::HasHooks);
        assert_eq!(merge_state("???"), MergeState::Unknown);
        assert_eq!(
            review_decision(Some("REVIEW_REQUIRED")),
            ReviewDecision::ReviewRequired
        );
        assert_eq!(review_decision(None), ReviewDecision::None);
        assert_eq!(pr_state("MERGED"), Some(PrState::Merged));
        assert_eq!(pr_state("DRAFT"), None);
        for (method, name) in MERGE_METHODS {
            assert_eq!(merge_method_name(method), name);
            assert_eq!(merge_method(name), Some(method));
        }
        assert_eq!(merge_method("FAST_FORWARD"), None);
    }
}
