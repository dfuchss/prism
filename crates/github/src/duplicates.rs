//! Detects pull requests that bump the same dependency as another one.

use crate::bump::compare_versions;
use protocol::{PrItem, PrRef, PrState};
use std::collections::HashMap;

/// Flags superseded bump pull requests. Within one repository, PRs whose bump names the same
/// dependency (case-insensitive) in the same directory form a group; the PR with the highest
/// target version is kept (ties: the newest `created_at`, then the highest number) and every other one gets
/// `duplicate_of = Some(kept)`. All other items get `duplicate_of = None`, so the function can
/// be re-run on a refreshed list. Merged and closed PRs take no part.
pub fn mark_duplicates(items: &mut [PrItem]) {
    type Key = (String, String, Option<String>);
    let mut groups: HashMap<Key, Vec<(usize, String)>> = HashMap::new();
    for (index, item) in items.iter_mut().enumerate() {
        item.duplicate_of = None;
        if item.state != PrState::Open {
            continue;
        }
        if let Some(bump) = &item.bump {
            let key = (
                item.pr.repo.clone(),
                bump.dependency.to_lowercase(),
                bump.directory.clone(),
            );
            groups
                .entry(key)
                .or_default()
                .push((index, bump.to.clone()));
        }
    }
    for group in groups.values().filter(|group| group.len() > 1) {
        let kept = group
            .iter()
            .max_by(|(a, to_a), (b, to_b)| {
                compare_versions(to_a, to_b)
                    .then_with(|| items[*a].created_at.cmp(&items[*b].created_at))
                    .then_with(|| items[*a].pr.number.cmp(&items[*b].pr.number))
            })
            .map(|(index, _)| *index);
        let Some(kept) = kept else { continue };
        let kept_ref: PrRef = items[kept].pr.clone();
        for (index, _) in group.iter().filter(|(index, _)| *index != kept) {
            items[*index].duplicate_of = Some(kept_ref.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bump::parse_bump;

    /// A PR whose bump is parsed from its title, as the details query does.
    fn item(repo: &str, number: u64, title: &str) -> PrItem {
        let mut item = PrItem::sample(repo, number, title);
        item.bump = parse_bump(title);
        item
    }

    const REPO: &str = "acme/widget";

    fn dup(items: &[PrItem], number: u64) -> Option<u64> {
        let found = items.iter().find(|i| i.pr.number == number).unwrap();
        found.duplicate_of.as_ref().map(|pr| pr.number)
    }

    #[test]
    fn merged_prs_take_no_part() {
        let mut items = vec![
            item(REPO, 2, "Bump foo from 1.0.0 to 1.1.0"),
            item(REPO, 1, "Bump foo from 1.0.0 to 1.0.5"),
            item(REPO, 3, "Bump foo from 1.0.0 to 2.0.0"),
        ];
        items[2].state = PrState::Merged;
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 1), Some(2));
        assert_eq!(dup(&items, 2), None);
        assert_eq!(dup(&items, 3), None);
    }

    #[test]
    fn higher_target_version_wins() {
        let mut items = vec![
            item(
                REPO,
                227,
                "Update dependency org.commonmark:commonmark to v0.33.0",
            ),
            item(
                REPO,
                220,
                "Dependency(deps): Bump org.commonmark:commonmark from 0.31.0 to 0.32.0",
            ),
        ];
        items[1].created_at = "2026-02-01T00:00:00Z".to_owned();
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 220), Some(227));
        assert_eq!(dup(&items, 227), None);
        assert_eq!(items[0].duplicate_of, None);
    }

    #[test]
    fn same_version_keeps_newest() {
        let mut items = vec![
            item(
                REPO,
                217,
                "Update dependency io.ktor:ktor-client-java-jvm to v3.6.1",
            ),
            item(
                REPO,
                219,
                "Bump io.ktor:ktor-client-java-jvm from 3.5.2 to 3.6.1",
            ),
        ];
        items[0].created_at = "2026-03-01T00:00:00Z".to_owned();
        items[1].created_at = "2026-03-05T00:00:00Z".to_owned();
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 217), Some(219));
        assert_eq!(dup(&items, 219), None);
    }

    #[test]
    fn different_repos_and_dependencies_are_not_duplicates() {
        let mut items = vec![
            item("a/x", 1, "Bump serde from 1.0.1 to 1.0.2"),
            item("a/y", 1, "Bump serde from 1.0.0 to 1.0.1"),
            item("a/x", 2, "Bump serde_json from 1.0.1 to 1.0.2"),
            item("a/x", 3, "Fix things"),
            item("a/x", 4, "Bump react and @types/react"),
        ];
        mark_duplicates(&mut items);
        assert!(items.iter().all(|i| i.duplicate_of.is_none()));
    }

    #[test]
    fn different_directories_are_not_duplicates() {
        let mut items = vec![
            item("a/x", 1, "Bump serde from 1.0.1 to 1.0.2 in /a"),
            item("a/x", 2, "Bump serde from 1.0.1 to 1.0.3 in /b"),
            item("a/x", 3, "Bump serde from 1.0.0 to 1.0.1 in /a"),
        ];
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 1), None);
        assert_eq!(dup(&items, 2), None);
        assert_eq!(dup(&items, 3), Some(1));
    }

    #[test]
    fn case_insensitive_three_way_and_rerun() {
        let mut items = vec![
            item("a/x", 1, "Bump Serde from 1.0.0 to 1.0.1"),
            item("a/x", 2, "Update dependency serde to v1.0.3"),
            item("a/x", 3, "Bump serde from 1.0.1 to 1.0.2"),
        ];
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 1), Some(2));
        assert_eq!(dup(&items, 3), Some(2));
        assert_eq!(dup(&items, 2), None);
        items[1] = item("a/x", 2, "Fix");
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 1), Some(3));
        assert_eq!(dup(&items, 3), None);
        assert_eq!(dup(&items, 2), None);
    }

    #[test]
    fn the_bump_not_the_title_counts() {
        let mut items = vec![
            item("a/x", 1, "Some renamed title"),
            item("a/x", 2, "Bump serde from 1.0.1 to 1.0.2"),
        ];
        items[0].bump = parse_bump("Bump serde from 1.0.2 to 1.0.5");
        mark_duplicates(&mut items);
        assert_eq!(dup(&items, 2), Some(1));
    }
}
