//! Local knowledge that GitHub's (eventually consistent) endpoints do not show yet: threads
//! marked done and pull requests merged or closed from here. A refresh applies it to what it
//! fetched, for [`TTL`], so a just-hidden item does not come back.

use protocol::{PrItem, PrRef, PrState};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long local knowledge outlives the action.
const TTL: Duration = Duration::from_mins(10);

#[derive(Default)]
pub(crate) struct Overrides {
    /// Thread id to when it was hidden (marked done, unsubscribed) and its `updated_at` then.
    done_threads: HashMap<String, (Instant, String)>,
    /// PR to the state set locally, when, and its `updated_at` then.
    states: HashMap<PrRef, (PrState, Instant, String)>,
}

impl Overrides {
    /// Keeps the notification thread hidden (it was marked done or unsubscribed here).
    pub(crate) fn hide_thread(&mut self, thread_id: &str, updated_at: &str, now: Instant) {
        self.done_threads
            .insert(thread_id.to_owned(), (now, updated_at.to_owned()));
    }

    /// Keeps the PR merged or closed (it was merged or closed here).
    pub(crate) fn remember_state(
        &mut self,
        pr: &PrRef,
        state: PrState,
        updated_at: &str,
        now: Instant,
    ) {
        self.states
            .insert(pr.clone(), (state, now, updated_at.to_owned()));
    }

    /// Applies the overrides to freshly fetched items. A hidden thread stays hidden unless it
    /// got newer activity; a PR GitHub still reports open (with no newer `updated_at`) keeps its
    /// local merged or closed state.
    pub(crate) fn apply(&mut self, items: &mut Vec<PrItem>, now: Instant) {
        let fresh = |when: &Instant| now.saturating_duration_since(*when) < TTL;
        self.done_threads.retain(|_, (when, _)| fresh(when));
        self.states.retain(|_, (_, when, _)| fresh(when));
        items.retain_mut(|item| {
            let stale_state = self
                .states
                .get(&item.pr)
                .filter(|(_, _, seen)| item.state == PrState::Open && item.updated_at <= *seen);
            if let Some((state, _, _)) = stale_state
                && !item.settle(*state)
            {
                return false;
            }
            let hidden = item.notification.as_ref().is_some_and(|n| {
                self.done_threads
                    .get(&n.thread_id)
                    .is_some_and(|(_, seen)| n.updated_at <= *seen)
            });
            !hidden || item.drop_notification()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{NotificationInfo, NotifyReason};

    fn pr_ref(number: u64) -> PrRef {
        PrRef {
            repo: "a/b".to_owned(),
            number,
        }
    }

    fn item(number: u64, thread: &str, updated: &str) -> PrItem {
        let mut item = PrItem::sample("a/b", number, "t");
        item.updated_at = updated.to_owned();
        item.notification = Some(NotificationInfo {
            thread_id: thread.to_owned(),
            reason: NotifyReason::Mention,
            unread: true,
            updated_at: updated.to_owned(),
        });
        item
    }

    #[test]
    fn done_threads_stay_hidden_until_new_activity_or_expiry() {
        let now = Instant::now();
        let mut overrides = Overrides::default();
        overrides.hide_thread("t1", "2026-01-01", now);
        let mut items = vec![item(1, "t1", "2026-01-01"), item(2, "t2", "2026-01-01")];
        overrides.apply(&mut items, now);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].pr.number, 2);

        let mut newer = vec![item(1, "t1", "2026-01-02")];
        overrides.apply(&mut newer, now);
        assert_eq!(newer.len(), 1);

        let mut late = vec![item(1, "t1", "2026-01-01")];
        overrides.apply(&mut late, now + TTL);
        assert_eq!(late.len(), 1);
    }

    #[test]
    fn done_keeps_items_with_another_source() {
        let now = Instant::now();
        let mut overrides = Overrides::default();
        overrides.hide_thread("t1", "x", now);
        let mut mine = item(1, "t1", "x");
        mine.is_mine = true;
        let mut items = vec![mine];
        overrides.apply(&mut items, now);
        assert_eq!(items.len(), 1);
        assert!(items[0].notification.is_none());
    }

    #[test]
    fn local_state_survives_a_lagging_fetch() {
        let now = Instant::now();
        let mut overrides = Overrides::default();
        let pr = pr_ref(1);
        overrides.remember_state(&pr, PrState::Merged, "t1", now);
        let mut lagging = vec![item(1, "a", "t1")];
        overrides.apply(&mut lagging, now);
        assert_eq!(lagging[0].state, PrState::Merged);

        let mut newer = vec![item(1, "a", "t2")];
        overrides.apply(&mut newer, now);
        assert_eq!(newer[0].state, PrState::Open);

        let mut plain = item(1, "a", "t1");
        plain.notification = None;
        let mut gone = vec![plain];
        overrides.apply(&mut gone, now);
        assert!(gone.is_empty());
    }
}
