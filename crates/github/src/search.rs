//! GraphQL search for open PRs awaiting the viewer's review or authored by the viewer.

use crate::client::Client;
use crate::error::Result;
use crate::gql::next_cursor;
use protocol::PrRef;
use serde_json::{Value, json};
use std::collections::HashMap;

const REVIEW_REQUESTED_QUERY: &str = "is:open is:pr review-requested:@me archived:false";
const AUTHORED_QUERY: &str = "is:open is:pr author:@me archived:false";

const SEARCH_GRAPHQL: &str = "query($q: String!, $after: String) { \
    search(query: $q, type: ISSUE, first: 100, after: $after) { \
    pageInfo { hasNextPage endCursor } \
    nodes { ... on PullRequest { number repository { nameWithOwner } } } } }";

/// A search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchHit {
    pub pr: PrRef,
    /// Found by the `author:@me` search.
    pub is_mine: bool,
    /// Found by the `review-requested:@me` search.
    pub review_requested: bool,
}

impl Client {
    /// Open PRs awaiting my review or authored by me (both searches run concurrently).
    pub(crate) async fn search_open_prs(&self) -> Result<Vec<SearchHit>> {
        let (requested, authored) = tokio::try_join!(
            self.search_all(REVIEW_REQUESTED_QUERY),
            self.search_all(AUTHORED_QUERY)
        )?;
        Ok(merge_hits(requested, authored))
    }

    async fn search_all(&self, query: &str) -> Result<Vec<PrRef>> {
        let variables = json!({ "q": query, "after": null });
        self.paginate(SEARCH_GRAPHQL, variables, parse_search_page)
            .await
    }
}

/// PRs of one result page and the cursor of the next page, if any.
fn parse_search_page(data: &Value) -> (Vec<PrRef>, Option<String>) {
    let search = &data["search"];
    let prs = search["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some(PrRef {
                repo: node["repository"]["nameWithOwner"].as_str()?.to_owned(),
                number: node["number"].as_u64()?,
            })
        })
        .collect();
    (prs, next_cursor(&search["pageInfo"]))
}

/// Merges the two searches in order of appearance; a PR found by both is both.
fn merge_hits(review_requested: Vec<PrRef>, authored: Vec<PrRef>) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    let mut positions: HashMap<PrRef, usize> = HashMap::new();
    for (prs, is_mine) in [(review_requested, false), (authored, true)] {
        for pr in prs {
            let index = *positions.entry(pr.clone()).or_insert_with(|| {
                hits.push(SearchHit {
                    pr,
                    is_mine: false,
                    review_requested: false,
                });
                hits.len() - 1
            });
            let hit = &mut hits[index];
            hit.is_mine |= is_mine;
            hit.review_requested |= !is_mine;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_page_and_cursor() {
        let data: Value =
            serde_json::from_str(include_str!("../tests/fixtures/search.json")).unwrap();
        let (prs, next) = parse_search_page(&data["data"]);
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].repo, "acme/widget");
        assert_eq!(next.as_deref(), Some("CURSOR1"));
    }

    #[test]
    fn merges_and_flags_mine() {
        let pr = |n| PrRef {
            repo: "a/b".to_owned(),
            number: n,
        };
        let hits = merge_hits(vec![pr(1), pr(2)], vec![pr(2), pr(3)]);
        let flags: Vec<_> = hits
            .iter()
            .map(|h| (h.pr.number, h.is_mine, h.review_requested))
            .collect();
        assert_eq!(flags, [(1, false, true), (2, true, true), (3, true, false)]);
    }
}
