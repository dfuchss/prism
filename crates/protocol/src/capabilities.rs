//! What the viewer may do to a pull request, as far as GitHub tells, and why an action is not
//! available. Unknown data is treated as allowed: GitHub stays the final judge.

use crate::action::Action;
use crate::inbox::PrItem;
use serde::{Deserialize, Serialize};

/// Whether auto-merge can be enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoMerge {
    /// The reason is shown in tooltips and dialogs.
    Unavailable(String),
    Available,
    AlreadyEnabled,
}

/// The viewer's possibilities on one pull request (from the details query).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The repository is archived (read-only).
    pub archived: bool,
    /// Approving is possible (not archived; the own-PR check needs `PrItem::is_mine`).
    pub can_approve: bool,
    /// Write access (or more) and not archived.
    pub can_merge: bool,
    pub auto_merge: AutoMerge,
    /// GitHub's branch update; bot rebases by comment do not need it.
    pub can_update_branch: bool,
    pub can_close: bool,
    pub can_comment: bool,
}

impl Default for Capabilities {
    /// Everything allowed: the state before (or without) the details are known.
    fn default() -> Self {
        Self {
            archived: false,
            can_approve: true,
            can_merge: true,
            auto_merge: AutoMerge::Available,
            can_update_branch: true,
            can_close: true,
            can_comment: true,
        }
    }
}

const NO_WRITE: &str = "You need write access to this repository";
const ARCHIVED: &str = "This repository is archived";

/// Why `action` cannot work on `item`; `None` when it can (or is not restricted). Used by the
/// UI (disabled buttons), the planner and the executor.
pub fn unavailable_reason(item: &PrItem, action: &Action) -> Option<String> {
    let caps = &item.caps;
    let merge =
        || (!caps.can_merge).then(|| if caps.archived { ARCHIVED } else { NO_WRITE }.to_owned());
    let approve = || {
        if item.is_mine {
            Some("You cannot approve your own pull request".to_owned())
        } else {
            (!caps.can_approve).then(|| ARCHIVED.to_owned())
        }
    };
    match action {
        Action::Approve => approve(),
        Action::Merge(_) => merge(),
        Action::ApproveAndMerge(_) => approve().or_else(merge),
        Action::EnableAutoMerge(_) => merge().or(match &caps.auto_merge {
            AutoMerge::Unavailable(reason) => Some(reason.clone()),
            AutoMerge::AlreadyEnabled => Some("Auto-merge is already enabled".to_owned()),
            AutoMerge::Available => None,
        }),
        Action::Close { .. } => (!caps.can_close).then(|| {
            if caps.archived {
                ARCHIVED.to_owned()
            } else {
                "You cannot close this pull request".to_owned()
            }
        }),
        Action::Comment(_) => (!caps.can_comment).then(|| ARCHIVED.to_owned()),
        Action::Rebase => {
            let by_bot = matches!(item.author.as_str(), "dependabot[bot]" | "renovate[bot]");
            (!caps.can_update_branch && !by_bot)
                .then(|| "You cannot update this pull request's branch".to_owned())
        }
        Action::MarkRead | Action::MarkDone | Action::Unsubscribe | Action::RerunFailed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(change: impl FnOnce(&mut PrItem)) -> PrItem {
        let mut pr = PrItem::sample("acme/widget", 1, "Bump serde from 1.0.1 to 1.0.2");
        change(&mut pr);
        pr
    }

    fn reason(pr: &PrItem, action: &Action) -> String {
        unavailable_reason(pr, action).unwrap_or_default()
    }

    #[test]
    fn everything_is_available_by_default() {
        let pr = with(|_| {});
        for action in [
            Action::Approve,
            Action::Merge(None),
            Action::ApproveAndMerge(None),
            Action::EnableAutoMerge(None),
            Action::Close { comment: None },
            Action::Comment("x".to_owned()),
            Action::Rebase,
            Action::MarkDone,
        ] {
            assert_eq!(unavailable_reason(&pr, &action), None, "{action:?}");
        }
    }

    #[test]
    fn own_pr_cannot_be_approved_but_merged() {
        let pr = with(|p| p.is_mine = true);
        assert!(reason(&pr, &Action::Approve).contains("own"));
        assert!(reason(&pr, &Action::ApproveAndMerge(None)).contains("own"));
        assert_eq!(unavailable_reason(&pr, &Action::Merge(None)), None);
        assert_eq!(
            unavailable_reason(&pr, &Action::EnableAutoMerge(None)),
            None
        );
    }

    #[test]
    fn missing_write_access_blocks_merging() {
        let pr = with(|p| p.caps.can_merge = false);
        for action in [
            Action::Merge(None),
            Action::ApproveAndMerge(None),
            Action::EnableAutoMerge(None),
        ] {
            assert!(reason(&pr, &action).contains("write access"), "{action:?}");
        }
        assert_eq!(unavailable_reason(&pr, &Action::Approve), None);
    }

    #[test]
    fn auto_merge_states() {
        let off = with(|p| {
            p.caps.auto_merge = AutoMerge::Unavailable("disabled in settings".to_owned());
        });
        assert_eq!(
            reason(&off, &Action::EnableAutoMerge(None)),
            "disabled in settings"
        );
        assert_eq!(unavailable_reason(&off, &Action::Merge(None)), None);
        let on = with(|p| p.caps.auto_merge = AutoMerge::AlreadyEnabled);
        assert!(reason(&on, &Action::EnableAutoMerge(None)).contains("already"));
    }

    #[test]
    fn archived_repository_blocks_most_actions() {
        let pr = with(|p| {
            p.caps = Capabilities {
                archived: true,
                can_approve: false,
                can_merge: false,
                can_close: false,
                can_comment: false,
                ..Capabilities::default()
            };
        });
        for action in [
            Action::Approve,
            Action::Merge(None),
            Action::Close { comment: None },
            Action::Comment("x".to_owned()),
        ] {
            assert!(reason(&pr, &action).contains("archived"), "{action:?}");
        }
        assert_eq!(unavailable_reason(&pr, &Action::MarkRead), None);
    }

    #[test]
    fn rebase_needs_branch_update_unless_a_bot_does_it() {
        let human = with(|p| {
            p.author = "alice".to_owned();
            p.caps.can_update_branch = false;
        });
        assert!(reason(&human, &Action::Rebase).contains("branch"));
        let bot = with(|p| p.caps.can_update_branch = false);
        assert_eq!(unavailable_reason(&bot, &Action::Rebase), None);
    }
}
