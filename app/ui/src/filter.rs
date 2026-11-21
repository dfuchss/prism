//! Which pull requests the inbox shows: buckets, repositories, toggles and search.

use protocol::{Bucket, PrItem, PrState, bucket};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum View {
    #[default]
    All,
    Duplicates,
    Bucket(Bucket),
    /// Merged and closed pull requests (they have a notification).
    Closed,
}

impl View {
    /// Sidebar order.
    pub(crate) const ALL: [Self; 9] = [
        Self::All,
        Self::Bucket(Bucket::Ready),
        Self::Bucket(Bucket::NeedsApproval),
        Self::Bucket(Bucket::Failing),
        Self::Bucket(Bucket::Pending),
        Self::Bucket(Bucket::Mine),
        Self::Bucket(Bucket::Draft),
        Self::Duplicates,
        Self::Closed,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Duplicates => "Duplicates",
            Self::Closed => "Merged/Closed",
            Self::Bucket(Bucket::Mine) => "Mine",
            Self::Bucket(Bucket::Ready) => "Ready",
            Self::Bucket(Bucket::NeedsApproval) => "Needs approval",
            Self::Bucket(Bucket::Failing) => "Failing",
            Self::Bucket(Bucket::Pending) => "Pending",
            Self::Bucket(Bucket::Draft) => "Draft",
        }
    }

    fn contains(self, item: &PrItem) -> bool {
        let open = item.state == PrState::Open;
        match self {
            Self::Closed => !open,
            Self::All => open,
            Self::Duplicates => open && item.duplicate_of.is_some(),
            Self::Bucket(wanted) => open && bucket(item) == wanted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Who {
    #[default]
    Everyone,
    Bots,
    Humans,
}

/// Which repositories the inbox shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum RepoFilter {
    #[default]
    All,
    /// Every repository of an owner (organization or user).
    Owner(String),
    /// One repository, `owner/name`.
    Repo(String),
}

/// The owner of an `owner/name` repository.
pub(crate) fn owner_of(repo: &str) -> &str {
    repo.split_once('/').map_or(repo, |(owner, _)| owner)
}

impl RepoFilter {
    pub(crate) fn accepts(&self, repo: &str) -> bool {
        match self {
            Self::All => true,
            Self::Owner(owner) => owner_of(repo) == owner,
            Self::Repo(wanted) => wanted == repo,
        }
    }

    /// The filter to keep for `items`: one that selects an owner or repository without any item
    /// (e.g. after a settings change removed it) falls back to all repositories.
    #[must_use]
    pub(crate) fn pruned(self, items: &[PrItem]) -> Self {
        let present = match &self {
            Self::All => true,
            Self::Owner(_) | Self::Repo(_) => items.iter().any(|item| self.accepts(&item.pr.repo)),
        };
        if present { self } else { Self::All }
    }

    /// The filter after clicking the entry `target`: clicking the active entry clears it.
    #[must_use]
    pub(crate) fn clicked(&self, target: Self) -> Self {
        if *self == target { Self::All } else { target }
    }
}

/// Part of the filter to leave out, to count what a sidebar entry would show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    Nothing,
    View,
    Repo,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Filter {
    pub view: View,
    pub repo: RepoFilter,
    pub unread_only: bool,
    /// Hide PRs that are in the inbox only through the repository scope.
    pub involving_me_only: bool,
    pub who: Who,
    pub search: String,
}

impl Filter {
    pub(crate) fn accepts(&self, item: &PrItem, skip: Skip) -> bool {
        (skip == Skip::View || self.view.contains(item))
            && (skip == Skip::Repo || self.repo.accepts(&item.pr.repo))
            && self.flags_accept(item)
            && self.search_accepts(item)
    }

    fn flags_accept(&self, item: &PrItem) -> bool {
        let unread = item.notification.as_ref().is_some_and(|n| n.unread);
        (!self.unread_only || unread)
            && !(self.involving_me_only && item.via_scope_only())
            && match self.who {
                Who::Everyone => true,
                Who::Bots => item.is_bot,
                Who::Humans => !item.is_bot,
            }
    }

    fn search_accepts(&self, item: &PrItem) -> bool {
        let needle = self.search.trim().to_lowercase();
        needle.is_empty()
            || [
                item.title.as_str(),
                item.pr.repo.as_str(),
                item.author.as_str(),
            ]
            .iter()
            .any(|text| text.to_lowercase().contains(&needle))
            || format!("#{}", item.pr.number).contains(&needle)
    }
}

/// The matching items, most recently updated first.
pub(crate) fn visible(items: &[PrItem], filter: &Filter) -> Vec<PrItem> {
    let mut shown: Vec<PrItem> = items
        .iter()
        .filter(|item| filter.accepts(item, Skip::Nothing))
        .cloned()
        .collect();
    shown.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    shown
}

pub(crate) fn view_counts(items: &[PrItem], filter: &Filter) -> Vec<(View, usize)> {
    View::ALL
        .into_iter()
        .map(|view| {
            let count = items
                .iter()
                .filter(|item| filter.accepts(item, Skip::View) && view.contains(item))
                .count();
            (view, count)
        })
        .collect()
}

/// An owner with its number of matching items and its repositories (busiest first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerCount {
    pub owner: String,
    pub count: usize,
    pub repos: Vec<(String, usize)>,
}

/// Owners with their repositories and counts, busiest first. The repository filter itself is
/// left out, so every entry shows what clicking it would give.
pub(crate) fn owner_counts(items: &[PrItem], filter: &Filter) -> Vec<OwnerCount> {
    let mut owners: Vec<OwnerCount> = Vec::new();
    for item in items.iter().filter(|item| filter.accepts(item, Skip::Repo)) {
        let owner = owner_of(&item.pr.repo);
        let index = owners
            .iter()
            .position(|o| o.owner == owner)
            .unwrap_or_else(|| {
                owners.push(OwnerCount {
                    owner: owner.to_owned(),
                    count: 0,
                    repos: Vec::new(),
                });
                owners.len() - 1
            });
        let entry = &mut owners[index];
        entry.count += 1;
        match entry
            .repos
            .iter_mut()
            .find(|(repo, _)| *repo == item.pr.repo)
        {
            Some((_, count)) => *count += 1,
            None => entry.repos.push((item.pr.repo.clone(), 1)),
        }
    }
    for owner in &mut owners {
        owner
            .repos
            .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }
    owners.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.owner.cmp(&b.owner)));
    owners
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(repo: &str) -> PrItem {
        let mut item = PrItem::sample(repo, 1, "t");
        item.is_bot = false;
        item
    }

    #[test]
    fn repo_filter_by_owner_and_repo_and_toggle() {
        let all = RepoFilter::All;
        assert!(all.accepts("a/b"));
        let owner = RepoFilter::Owner("acme".to_owned());
        assert!(owner.accepts("acme/x") && owner.accepts("acme/y"));
        assert!(!owner.accepts("acme-corp/x") && !owner.accepts("other/acme"));
        let repo = RepoFilter::Repo("acme/x".to_owned());
        assert!(repo.accepts("acme/x") && !repo.accepts("acme/y"));
        assert_eq!(all.clicked(owner.clone()), owner);
        assert_eq!(owner.clicked(owner.clone()), RepoFilter::All);
        assert_eq!(owner.clicked(repo.clone()), repo);
        assert_eq!(repo.clicked(repo.clone()), RepoFilter::All);
    }

    #[test]
    fn filter_of_a_vanished_repo_resets_to_all() {
        let items = [sample("acme/x"), sample("other/z")];
        let repo = RepoFilter::Repo("acme/x".to_owned());
        assert_eq!(repo.clone().pruned(&items), repo);
        assert_eq!(repo.pruned(&items[1..]), RepoFilter::All);
        let owner = RepoFilter::Owner("acme".to_owned());
        assert_eq!(owner.clone().pruned(&items), owner);
        assert_eq!(owner.pruned(&[]), RepoFilter::All);
        assert_eq!(RepoFilter::All.pruned(&[]), RepoFilter::All);
    }

    #[test]
    fn owner_counts_follow_the_other_filters() {
        let mut bot = sample("acme/x");
        bot.is_bot = true;
        let items = [bot.clone(), bot, sample("acme/y"), sample("other/z")];
        let mut filter = Filter::default();
        let counts = owner_counts(&items, &filter);
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0].owner, "acme");
        assert_eq!(counts[0].count, 3);
        assert_eq!(
            counts[0].repos,
            [("acme/x".to_owned(), 2), ("acme/y".to_owned(), 1)]
        );
        // The repository filter does not shrink the counts; the bot toggle does.
        filter.repo = RepoFilter::Repo("other/z".to_owned());
        assert_eq!(owner_counts(&items, &filter).len(), 2);
        filter.who = Who::Humans;
        assert_eq!(owner_counts(&items, &filter)[0].count, 1);
    }

    #[test]
    fn non_open_items_only_show_in_merged_closed() {
        let mut item = sample("a/b");
        let items = [item.clone()];
        let counts =
            |items: &[PrItem]| -> Vec<(View, usize)> { view_counts(items, &Filter::default()) };
        assert!(counts(&items).contains(&(View::All, 1)));
        assert!(counts(&items).contains(&(View::Closed, 0)));
        item.state = PrState::Merged;
        let items = [item];
        for (view, count) in counts(&items) {
            assert_eq!(count, usize::from(view == View::Closed), "{view:?}");
        }
        assert!(visible(&items, &Filter::default()).is_empty());
    }
}
