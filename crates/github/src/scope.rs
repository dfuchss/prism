//! Repositories the viewer can access and their open pull requests (the `settings.toml` scope).

use crate::client::Client;
use crate::error::Result;
use crate::gql::{next_cursor, owner_and_name, quote};
use protocol::PrRef;
use serde_json::{Value, json};

/// Repositories per open-PR request. Kept small like the details chunks: big requests made
/// GitHub answer with HTTP 502.
const REPOS_PER_REQUEST: usize = 20;

const REPOS_GRAPHQL: &str = "query($after: String) { viewer { \
    repositories(first: 100, after: $after, isArchived: false, \
    affiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER], \
    ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER]) { \
    pageInfo { hasNextPage endCursor } nodes { nameWithOwner } } } }";

/// A repository whose open PRs are listed, from the page after `after` when set.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoPage {
    repo: String,
    after: Option<String>,
}

/// Repository names of one result page and the cursor of the next page, if any.
fn parse_repos_page(data: &Value) -> (Vec<String>, Option<String>) {
    let repositories = &data["viewer"]["repositories"];
    let repos = repositories["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["nameWithOwner"].as_str().map(str::to_owned))
        .collect();
    (repos, next_cursor(&repositories["pageInfo"]))
}

/// Builds the open-PR query for one chunk; alias `r{i}` belongs to `repos[i]`.
fn build_open_prs_query(repos: &[RepoPage]) -> String {
    let aliases: String = repos
        .iter()
        .enumerate()
        .map(|(index, page)| alias(index, page))
        .collect();
    format!("query {{ {aliases} }}")
}

fn alias(index: usize, page: &RepoPage) -> String {
    let (owner, name) = owner_and_name(&page.repo);
    let after = page
        .after
        .as_deref()
        .map_or_else(String::new, |cursor| format!(", after: {}", quote(cursor)));
    format!(
        "r{index}: repository(owner: {}, name: {}) {{ \
         pullRequests(states: OPEN, first: 100{after}) {{ nodes {{ number }} \
         pageInfo {{ hasNextPage endCursor }} }} }} ",
        quote(owner),
        quote(name)
    )
}

/// Open PRs found for `repos` plus the repositories that have more pages (with their cursor).
/// Repositories that could not be read (null) are skipped.
fn parse_open_prs(data: &Value, repos: &[RepoPage]) -> (Vec<PrRef>, Vec<RepoPage>) {
    let mut prs = Vec::new();
    let mut more = Vec::new();
    for (index, RepoPage { repo, .. }) in repos.iter().enumerate() {
        let list = &data[format!("r{index}")]["pullRequests"];
        prs.extend(
            list["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|node| {
                    Some(PrRef {
                        repo: repo.clone(),
                        number: node["number"].as_u64()?,
                    })
                }),
        );
        if let Some(cursor) = next_cursor(&list["pageInfo"]) {
            more.push(RepoPage {
                repo: repo.clone(),
                after: Some(cursor),
            });
        }
    }
    (prs, more)
}

impl Client {
    /// `owner/name` of every non-archived repository the viewer owns, collaborates on or can
    /// reach as an organization member (all pages).
    pub async fn accessible_repos(&self) -> Result<Vec<String>> {
        self.paginate(REPOS_GRAPHQL, json!({ "after": null }), parse_repos_page)
            .await
    }

    /// Every open PR of `repos`, in chunks of [`REPOS_PER_REQUEST`]; repositories with more than
    /// 100 open PRs are paged.
    pub async fn open_pr_refs(&self, repos: &[String]) -> Result<Vec<PrRef>> {
        let mut pending: Vec<RepoPage> = repos
            .iter()
            .map(|repo| RepoPage {
                repo: repo.clone(),
                after: None,
            })
            .collect();
        let mut prs = Vec::new();
        while !pending.is_empty() {
            let mut next_round = Vec::new();
            for chunk in pending.chunks(REPOS_PER_REQUEST) {
                // Not strict: a repository GitHub will not show (e.g. SAML) is null plus an error.
                let data = self
                    .graphql_partial(&build_open_prs_query(chunk), json!({}))
                    .await?;
                let (found, more) = parse_open_prs(&data, chunk);
                prs.extend(found);
                next_round.extend(more);
            }
            pending = next_round;
        }
        Ok(prs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(repos: &[&str], next: Option<&str>) -> Value {
        json!({"viewer": {"repositories": {
            "nodes": repos.iter().map(|r| json!({"nameWithOwner": r})).collect::<Vec<_>>(),
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}
        }}})
    }

    fn page_of(repo: &str, after: Option<&str>) -> RepoPage {
        RepoPage {
            repo: repo.to_owned(),
            after: after.map(str::to_owned),
        }
    }

    #[test]
    fn parses_repository_pages() {
        let (repos, next) = parse_repos_page(&page(&["acme/widget", "acme/gadget"], Some("C1")));
        assert_eq!(repos, ["acme/widget", "acme/gadget"]);
        assert_eq!(next.as_deref(), Some("C1"));
        assert_eq!(parse_repos_page(&page(&[], None)), (vec![], None));
    }

    #[test]
    fn query_has_one_alias_per_repo_and_cursor() {
        let repos = [
            page_of("acme/widget", None),
            page_of("acme/gadget", Some("CUR")),
        ];
        let query = build_open_prs_query(&repos);
        assert!(query.contains(r#"r0: repository(owner: "acme", name: "widget")"#));
        assert!(query.contains(r#"r1: repository(owner: "acme", name: "gadget")"#));
        assert!(query.contains(r#"first: 100, after: "CUR""#));
        assert!(query.contains("states: OPEN"));
        assert_eq!(query.matches("after:").count(), 1);
    }

    #[test]
    fn parses_open_prs_missing_repos_and_more_pages() {
        let repos = ["acme/widget", "acme/hidden", "acme/busy"].map(|repo| page_of(repo, None));
        let data = json!({
            "r0": {"pullRequests": {"nodes": [{"number": 3}, {"number": 5}],
                "pageInfo": {"hasNextPage": false, "endCursor": "x"}}},
            "r1": null,
            "r2": {"pullRequests": {"nodes": [{"number": 9}],
                "pageInfo": {"hasNextPage": true, "endCursor": "NEXT"}}},
        });
        let (prs, more) = parse_open_prs(&data, &repos);
        let numbers: Vec<_> = prs.iter().map(|pr| (pr.repo.as_str(), pr.number)).collect();
        assert_eq!(
            numbers,
            [("acme/widget", 3), ("acme/widget", 5), ("acme/busy", 9)]
        );
        assert_eq!(more, [page_of("acme/busy", Some("NEXT"))]);
    }
}
