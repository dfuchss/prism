//! The repository scope: the cached list of accessible repositories and the open PRs in scope.

use crate::settings::Scope;
use crate::state::{AppState, lock};
use protocol::{InboxSnapshot, PrItem, PrRef, ScopePreview};
use std::time::{Duration, Instant};

/// How long the list of accessible repositories stays valid.
const REPO_CACHE_SECS: u64 = 3600;

/// The accessible repositories, fetched at most once per [`REPO_CACHE_SECS`].
#[derive(Default)]
pub(crate) struct RepoCache {
    fetched: Option<Instant>,
    repos: Vec<String>,
}

/// Whether a cache fetched at `fetched` is still valid at `now`.
fn is_fresh(fetched: Option<Instant>, now: Instant) -> bool {
    fetched.is_some_and(|at| now.duration_since(at) < Duration::from_secs(REPO_CACHE_SECS))
}

/// The accessible repositories: cached, refreshed at most hourly.
pub(crate) async fn accessible_repos(state: &AppState) -> Result<Vec<String>, String> {
    {
        let cache = lock(&state.repo_cache);
        if is_fresh(cache.fetched, Instant::now()) {
            return Ok(cache.repos.clone());
        }
    }
    let repos = state
        .client
        .accessible_repos()
        .await
        .map_err(|error| format!("could not list repositories: {error}"))?;
    let mut cache = lock(&state.repo_cache);
    cache.repos.clone_from(&repos);
    cache.fetched = Some(Instant::now());
    Ok(repos)
}

/// Forgets the cached repositories (when the settings changed).
pub(crate) fn invalidate(state: &AppState) {
    lock(&state.repo_cache).fetched = None;
}

/// Drops the items of excluded repositories, whatever brought them in.
pub(crate) fn drop_excluded(items: &mut Vec<PrItem>, scope: &Scope) {
    items.retain(|item| !scope.excludes(&item.pr.repo));
}

/// Applies a changed `scope` to the cached `snapshot` without fetching: excluded repositories
/// vanish, `in_scope` follows the globs, and items that stayed only because of the old scope
/// (no notification, not yours, no review request) are dropped. Repositories that became
/// covered have no items yet; the refresh after the change brings them.
pub(crate) fn rescope(snapshot: &mut InboxSnapshot, scope: &Scope) {
    drop_excluded(&mut snapshot.items, scope);
    for item in &mut snapshot.items {
        item.in_scope = scope.covers(&item.pr.repo);
    }
    snapshot
        .items
        .retain(|item| item.notification.is_some() || item.has_other_source());
}

/// The accessible repositories that `scope` covers and those it excludes.
pub(crate) async fn preview(state: &AppState, scope: &Scope) -> Result<ScopePreview, String> {
    let repos = accessible_repos(state).await?;
    Ok(ScopePreview {
        covered: scope.filter_repos(&repos),
        excluded: scope.excluded_repos(&repos),
    })
}

/// The accessible repositories that `scope` covers.
pub(crate) async fn repos_in_scope(state: &AppState, scope: &Scope) -> Result<Vec<String>, String> {
    if scope.is_empty() {
        return Ok(Vec::new());
    }
    Ok(scope.filter_repos(&accessible_repos(state).await?))
}

/// Open PRs of the repositories in the current scope; a failure is a message for the banner and
/// leaves the inbox without scope PRs.
pub(crate) async fn scope_refs(state: &AppState) -> (Vec<PrRef>, Option<String>) {
    let scope = lock(&state.settings).value.scope.clone();
    let repos = match repos_in_scope(state, &scope).await {
        Ok(repos) => repos,
        Err(error) => return (Vec::new(), Some(error)),
    };
    match state.client.open_pr_refs(&repos).await {
        Ok(refs) => (refs, None),
        Err(error) => (
            Vec::new(),
            Some(format!("could not list scope pull requests: {error}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{NotificationInfo, NotifyReason};

    fn item(repo: &str, number: u64) -> PrItem {
        let mut item = PrItem::sample(repo, number, "t");
        item.author = "bob".to_owned();
        item
    }

    fn notification() -> NotificationInfo {
        NotificationInfo {
            thread_id: "t".to_owned(),
            reason: NotifyReason::Mention,
            unread: true,
            updated_at: String::new(),
        }
    }

    fn scope(include: &[&str], exclude: &[&str]) -> Scope {
        Scope {
            include: include.iter().map(|g| (*g).to_owned()).collect(),
            exclude: exclude.iter().map(|g| (*g).to_owned()).collect(),
        }
    }

    fn snapshot(items: Vec<PrItem>) -> InboxSnapshot {
        InboxSnapshot {
            items,
            fetched_at: String::new(),
            error: None,
        }
    }

    fn numbers(snapshot: &InboxSnapshot) -> Vec<u64> {
        snapshot.items.iter().map(|item| item.pr.number).collect()
    }

    #[test]
    fn rescope_drops_uncovered_scope_only_items_and_keeps_the_rest() {
        let mut scoped = item("a/old", 1);
        scoped.in_scope = true;
        let mut mine = item("a/old", 2);
        mine.in_scope = true;
        mine.is_mine = true;
        let mut notified = item("a/old", 3);
        notified.in_scope = true;
        notified.notification = Some(notification());
        let mut snap = snapshot(vec![scoped, mine, notified]);
        rescope(&mut snap, &scope(&["a/new"], &[]));
        assert_eq!(numbers(&snap), [2, 3]);
        assert!(snap.items.iter().all(|i| !i.in_scope));
    }

    #[test]
    fn rescope_marks_notified_items_of_covered_repos() {
        let mut notified = item("a/x", 1);
        notified.notification = Some(notification());
        let mut snap = snapshot(vec![notified]);
        rescope(&mut snap, &scope(&["a/*"], &[]));
        assert!(snap.items[0].in_scope && !snap.items[0].via_scope_only());
        // Once the notification is done, the scope keeps it (as "via repo").
        assert!(snap.items[0].drop_notification());
        assert!(snap.items[0].via_scope_only());
    }

    #[test]
    fn exclude_hides_every_source() {
        let mut notified = item("kit/excluded", 1);
        notified.notification = Some(notification());
        let mut mine = item("kit/excluded", 2);
        mine.is_mine = true;
        let mut requested = item("kit/excluded", 3);
        requested.review_requested = true;
        let mut kept = item("kit/other", 4);
        kept.is_mine = true;
        let items = vec![notified, mine, requested, kept];
        let scope = scope(&["kit/*"], &["kit/excluded"]);
        let mut fetched = items.clone();
        drop_excluded(&mut fetched, &scope);
        assert_eq!(fetched.len(), 1);
        let mut snap = snapshot(items);
        rescope(&mut snap, &scope);
        assert_eq!(numbers(&snap), [4]);
    }

    #[test]
    fn cache_is_fresh_for_an_hour() {
        let now = Instant::now();
        assert!(!is_fresh(None, now));
        assert!(is_fresh(Some(now), now));
        let old = now.checked_sub(Duration::from_secs(REPO_CACHE_SECS + 1));
        assert!(!is_fresh(old, now));
    }
}
