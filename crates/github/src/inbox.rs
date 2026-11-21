//! `inbox`: union of notifications, searches and the repository scope, enriched with details.

use crate::client::Client;
use crate::error::Result;
use crate::notifications::NotificationThread;
use crate::search::SearchHit;
use protocol::{InboxSnapshot, NotificationInfo, PrItem, PrRef, now_rfc3339};
use std::collections::{HashMap, HashSet};

impl Client {
    /// Fetches everything the inbox shows. `scope` are extra open PRs (from the repository
    /// scope). Unreadable PRs end up in `error` as warnings.
    pub async fn inbox(&self, scope: &[PrRef]) -> Result<InboxSnapshot> {
        let (viewer, threads, hits) = tokio::try_join!(
            self.viewer_login(),
            self.notifications(true),
            self.search_open_prs()
        )?;
        let refs = unique_refs(&threads, &hits, scope);
        let notified: HashSet<PrRef> = threads.iter().map(|thread| thread.pr.clone()).collect();
        let details = self.details(&refs, &notified).await?;
        let items = attach(details.items, &threads, &hits, &viewer, scope);
        Ok(InboxSnapshot {
            items,
            fetched_at: now_rfc3339(),
            error: (!details.warnings.is_empty()).then(|| details.warnings.join("; ")),
        })
    }
}

/// Every PR once: notifications first, then search hits, then the scope.
fn unique_refs(threads: &[NotificationThread], hits: &[SearchHit], scope: &[PrRef]) -> Vec<PrRef> {
    let mut seen: HashSet<&PrRef> = HashSet::new();
    threads
        .iter()
        .map(|thread| &thread.pr)
        .chain(hits.iter().map(|hit| &hit.pr))
        .chain(scope)
        .filter(|pr| seen.insert(pr))
        .cloned()
        .collect()
}

/// Sets the inbox sources of every item: `notification` (the first thread per PR), `is_mine`
/// (authored search hit or the viewer's login), `review_requested` and `in_scope`.
fn attach(
    mut items: Vec<PrItem>,
    threads: &[NotificationThread],
    hits: &[SearchHit],
    viewer: &str,
    scope: &[PrRef],
) -> Vec<PrItem> {
    let mut first_threads: HashMap<&PrRef, &NotificationInfo> = HashMap::new();
    for thread in threads {
        first_threads.entry(&thread.pr).or_insert(&thread.info);
    }
    let hits: HashMap<&PrRef, &SearchHit> = hits.iter().map(|hit| (&hit.pr, hit)).collect();
    let scope: HashSet<&PrRef> = scope.iter().collect();
    for item in &mut items {
        let hit = hits.get(&item.pr);
        item.notification = first_threads.get(&item.pr).map(|&info| info.clone());
        item.is_mine =
            hit.is_some_and(|hit| hit.is_mine) || item.author.eq_ignore_ascii_case(viewer);
        item.review_requested = hit.is_some_and(|hit| hit.review_requested);
        item.in_scope = scope.contains(&item.pr);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::NotifyReason;

    fn pr(number: u64) -> PrRef {
        PrRef {
            repo: "acme/widget".to_owned(),
            number,
        }
    }

    fn thread(number: u64, id: &str) -> NotificationThread {
        NotificationThread {
            pr: pr(number),
            info: NotificationInfo {
                thread_id: id.to_owned(),
                reason: NotifyReason::Author,
                unread: true,
                updated_at: String::new(),
            },
        }
    }

    fn requested(number: u64) -> SearchHit {
        SearchHit {
            pr: pr(number),
            is_mine: false,
            review_requested: true,
        }
    }

    fn item(number: u64, author: &str) -> PrItem {
        let mut item = PrItem::sample("acme/widget", number, "t");
        author.clone_into(&mut item.author);
        item
    }

    #[test]
    fn dedups_refs_and_attaches() {
        let threads = [thread(1, "a"), thread(1, "b"), thread(2, "c")];
        let hits = [requested(2)];
        assert_eq!(unique_refs(&threads, &hits, &[]), [pr(1), pr(2)]);
        let items = attach(
            vec![item(1, "Alice"), item(2, "bob")],
            &threads,
            &hits,
            "alice",
            &[],
        );
        assert_eq!(items[0].notification.as_ref().unwrap().thread_id, "a");
        assert!(items[0].is_mine && !items[0].review_requested);
        assert!(!items[1].is_mine && items[1].review_requested);
    }

    #[test]
    fn scope_prs_are_unioned_and_marked() {
        let threads = [thread(1, "a")];
        let hits = [requested(2)];
        let scope = [pr(1), pr(2), pr(3), pr(4)];
        assert_eq!(unique_refs(&threads, &hits, &scope), scope);
        let items = vec![
            item(1, "bob"),
            item(2, "bob"),
            item(3, "bob"),
            item(4, "alice"),
        ];
        let attached = attach(items, &threads, &hits, "alice", &scope[2..]);
        let marked: Vec<(bool, bool)> = attached
            .iter()
            .map(|item| (item.in_scope, item.via_scope_only()))
            .collect();
        // notification, review requested, only scope, own PR.
        assert_eq!(
            marked,
            [(false, false), (false, false), (true, true), (true, false)]
        );
    }
}
