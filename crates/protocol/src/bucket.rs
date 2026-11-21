//! Assigns pull requests to inbox buckets.

use crate::inbox::{Bucket, CiSummary, MergeState, PrItem, ReviewDecision};

/// Picks the bucket of a pull request; the first matching case wins:
///
/// 1. `Draft`: the PR is a draft (or GitHub reports the `DRAFT` merge state).
/// 2. `Mine`: it is my own PR.
/// 3. `Failing`: CI is failing, or the merge state is `Dirty` (conflicts). This precedes the
///    later cases, so an `Unstable` PR with failing checks is `Failing`.
/// 4. `Ready`: merge state `Clean` or `HasHooks` and CI green or absent.
/// 5. `NeedsApproval`: merge state `Blocked`, review decision `ReviewRequired` (CI is not
///    failing here, see 3).
/// 6. `Pending`: everything else (pending checks, `Unknown`, `Behind`, `Unstable`, ...).
pub fn bucket(item: &PrItem) -> Bucket {
    let ci = item.ci();
    if item.is_draft() {
        Bucket::Draft
    } else if item.is_mine {
        Bucket::Mine
    } else if ci == CiSummary::Failing || item.merge_state == MergeState::Dirty {
        Bucket::Failing
    } else if matches!(item.merge_state, MergeState::Clean | MergeState::HasHooks)
        && matches!(ci, CiSummary::Green | CiSummary::None)
    {
        Bucket::Ready
    } else if item.merge_state == MergeState::Blocked
        && item.review_decision == ReviewDecision::ReviewRequired
    {
        Bucket::NeedsApproval
    } else {
        Bucket::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckRun, CheckState};

    fn with(state: MergeState, review: ReviewDecision, ci: Option<CheckState>) -> PrItem {
        let mut pr = PrItem::sample("a/b", 1, "t");
        pr.merge_state = state;
        pr.review_decision = review;
        pr.checks = ci
            .map(|state| CheckRun {
                name: "ci".to_owned(),
                state,
                details_url: None,
                workflow_run_id: None,
            })
            .into_iter()
            .collect();
        pr
    }

    #[test]
    fn ready() {
        let none = ReviewDecision::None;
        let green = Some(CheckState::Success);
        assert_eq!(bucket(&with(MergeState::Clean, none, green)), Bucket::Ready);
        assert_eq!(bucket(&with(MergeState::Clean, none, None)), Bucket::Ready);
        assert_eq!(
            bucket(&with(MergeState::HasHooks, none, green)),
            Bucket::Ready
        );
    }

    #[test]
    fn failing() {
        let none = ReviewDecision::None;
        let red = Some(CheckState::Failure);
        assert_eq!(
            bucket(&with(MergeState::Unstable, none, red)),
            Bucket::Failing
        );
        assert_eq!(bucket(&with(MergeState::Clean, none, red)), Bucket::Failing);
        assert_eq!(
            bucket(&with(
                MergeState::Blocked,
                ReviewDecision::ReviewRequired,
                red
            )),
            Bucket::Failing
        );
        assert_eq!(
            bucket(&with(MergeState::Dirty, none, None)),
            Bucket::Failing
        );
    }

    #[test]
    fn needs_approval() {
        let required = ReviewDecision::ReviewRequired;
        let green = Some(CheckState::Success);
        assert_eq!(
            bucket(&with(MergeState::Blocked, required, green)),
            Bucket::NeedsApproval
        );
        assert_eq!(
            bucket(&with(
                MergeState::Blocked,
                required,
                Some(CheckState::Pending)
            )),
            Bucket::NeedsApproval
        );
        assert_eq!(
            bucket(&with(MergeState::Blocked, ReviewDecision::None, green)),
            Bucket::Pending
        );
    }

    #[test]
    fn pending() {
        let none = ReviewDecision::None;
        let wait = Some(CheckState::Pending);
        assert_eq!(
            bucket(&with(MergeState::Clean, none, wait)),
            Bucket::Pending
        );
        assert_eq!(
            bucket(&with(MergeState::Unstable, none, wait)),
            Bucket::Pending
        );
        assert_eq!(
            bucket(&with(MergeState::Unknown, none, None)),
            Bucket::Pending
        );
        assert_eq!(
            bucket(&with(MergeState::Behind, none, None)),
            Bucket::Pending
        );
    }

    #[test]
    fn draft_and_mine_come_first() {
        let mut pr = with(
            MergeState::Dirty,
            ReviewDecision::None,
            Some(CheckState::Failure),
        );
        pr.is_mine = true;
        assert_eq!(bucket(&pr), Bucket::Mine);
        pr.draft = true;
        assert_eq!(bucket(&pr), Bucket::Draft);
        let state = with(MergeState::Draft, ReviewDecision::None, None);
        assert_eq!(bucket(&state), Bucket::Draft);
    }
}
