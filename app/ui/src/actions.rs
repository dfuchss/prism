//! The manual actions the detail pane and the bulk bar offer, and why one is unavailable.

use protocol::{Action, PrItem, PrState, unavailable_reason};
use std::collections::BTreeMap;

/// What a click on an action button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    /// Runs right away (asks only when some PRs have to be skipped).
    Never,
    /// Asks for confirmation first.
    Confirm,
    /// Asks for confirmation with a comment box.
    Comment(CommentBox),
}

/// The comment box of a confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentBox {
    None,
    /// Closing: the text, if any, is posted before closing.
    Optional,
    /// Commenting: Confirm stays disabled while the text is empty.
    Required,
}

impl Ask {
    pub(crate) fn comment_box(self) -> CommentBox {
        match self {
            Self::Comment(comment) => comment,
            Self::Never | Self::Confirm => CommentBox::None,
        }
    }
}

/// One action button.
pub(crate) struct Offer {
    pub label: &'static str,
    pub action: Action,
    pub ask: Ask,
    pub danger: bool,
    /// Shown as a button in the floating selection bar; the others sit in its "More" menu.
    pub primary: bool,
}

const fn offer(label: &'static str, action: Action, ask: Ask, primary: bool) -> Offer {
    Offer {
        label,
        action,
        ask,
        danger: false,
        primary,
    }
}

/// Every manual action, in button order; the detail pane shows all of them, the selection bar
/// the `primary` ones and the rest in its menu (unavailable ones disabled, with the reason as
/// tooltip).
pub(crate) static OFFERS: [Offer; 11] = [
    offer("Approve", Action::Approve, Ask::Never, true),
    offer("Merge", Action::Merge(None), Ask::Confirm, true),
    offer(
        "Approve + Merge",
        Action::ApproveAndMerge(None),
        Ask::Confirm,
        true,
    ),
    offer(
        "Auto-merge",
        Action::EnableAutoMerge(None),
        Ask::Confirm,
        false,
    ),
    offer("Rebase", Action::Rebase, Ask::Never, true),
    offer("Rerun failed", Action::RerunFailed, Ask::Never, false),
    offer(
        "Comment...",
        Action::Comment(String::new()),
        Ask::Comment(CommentBox::Required),
        false,
    ),
    Offer {
        label: "Close...",
        action: Action::Close { comment: None },
        ask: Ask::Comment(CommentBox::Optional),
        danger: true,
        primary: true,
    },
    offer("Mark read", Action::MarkRead, Ask::Never, false),
    offer("Mark done", Action::MarkDone, Ask::Never, true),
    offer("Unsubscribe", Action::Unsubscribe, Ask::Never, false),
];

/// Why `action` cannot run on `item`: merged and closed PRs only take notification actions,
/// otherwise the viewer's capabilities decide.
pub(crate) fn reason(item: &PrItem, action: &Action) -> Option<String> {
    match item.state {
        PrState::Merged if !action.is_notification_only() => {
            Some("The pull request is merged".to_owned())
        }
        PrState::Closed if !action.is_notification_only() => {
            Some("The pull request is closed".to_owned())
        }
        _ => unavailable_reason(item, action),
    }
}

/// How many of `items` support `action`, and the distinct reasons of the others with their
/// number of PRs (sorted by reason).
pub(crate) fn availability<'a>(
    items: impl IntoIterator<Item = &'a PrItem>,
    action: &Action,
) -> (usize, BTreeMap<String, usize>) {
    let mut supported = 0;
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        match reason(item, action) {
            Some(reason) => *skipped.entry(reason).or_default() += 1,
            None => supported += 1,
        }
    }
    (supported, skipped)
}

/// The tooltip of a button: the reason when nothing supports it, the skipped ones when only
/// some do.
pub(crate) fn hint(supported: usize, skipped: &BTreeMap<String, usize>) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let reasons: Vec<&str> = skipped.keys().map(String::as_str).collect();
    Some(if supported == 0 {
        reasons.join("; ")
    } else {
        format!(
            "Some selected pull requests will be skipped: {}",
            reasons.join("; ")
        )
    })
}

/// `skipped: N - reason` lines for the confirm dialog.
pub(crate) fn skipped_lines(skipped: BTreeMap<String, usize>) -> Vec<String> {
    skipped
        .into_iter()
        .map(|(reason, count)| format!("skipped: {count} - {reason}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_prs_only_take_notification_actions() {
        let mut item = PrItem::sample("a/b", 1, "t");
        item.state = PrState::Merged;
        for offer in &OFFERS {
            let blocked = reason(&item, &offer.action).is_some();
            assert_eq!(
                blocked,
                !offer.action.is_notification_only(),
                "{}",
                offer.label
            );
        }
        item.state = PrState::Closed;
        assert!(reason(&item, &Action::Rebase).is_some_and(|r| r.contains("closed")));
    }

    #[test]
    fn availability_counts_reasons_and_hints() {
        let open = PrItem::sample("a/b", 1, "t");
        let mut mine = PrItem::sample("a/b", 2, "t");
        mine.is_mine = true;
        let (supported, skipped) = availability([&open, &mine, &mine], &Action::Approve);
        assert_eq!(supported, 1);
        assert_eq!(skipped.values().copied().collect::<Vec<_>>(), [2]);
        assert!(hint(supported, &skipped).is_some_and(|h| h.starts_with("Some selected")));
        let lines = skipped_lines(skipped.clone());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("skipped: 2 - "));
        let (supported, skipped) = availability([&mine], &Action::Approve);
        assert_eq!(
            hint(supported, &skipped).as_deref(),
            skipped.keys().next().map(String::as_str)
        );
        let (supported, skipped) = availability([&open], &Action::MarkDone);
        assert_eq!((supported, hint(supported, &skipped)), (1, None));
    }

    #[test]
    fn only_close_and_comment_ask_for_text() {
        for offer in &OFFERS {
            let wants_text = matches!(offer.action, Action::Close { .. } | Action::Comment(_));
            assert_eq!(
                offer.ask.comment_box() != CommentBox::None,
                wants_text,
                "{}",
                offer.label
            );
        }
    }

    #[test]
    fn the_bar_and_its_menu_split_the_offers() {
        let primary = OFFERS.iter().filter(|offer| offer.primary).count();
        assert!((1..=6).contains(&primary), "the bar must stay on one row");
        assert!(primary < OFFERS.len(), "the menu needs entries");
        assert!(OFFERS.iter().any(|offer| offer.primary && offer.danger));
    }
}
