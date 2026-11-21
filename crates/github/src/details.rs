//! PR details: an aliased GraphQL query per chunk of pull requests and its pure parsing.

use crate::bump::bump_of;
use crate::client::Client;
use crate::error::Result;
use crate::gql::{owner_and_name, quote};
use crate::mapping::{merge_method, merge_state, pr_state, review_decision};
use protocol::{
    AutoMerge, Capabilities, CheckRun, CheckState, MergeMethod, PrItem, PrRef, PrState,
};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Pull requests per GraphQL request. 40 with the review and rollup fields made GitHub time out
/// (HTTP 502) on a real inbox; 20 works.
const CHUNK_SIZE: usize = 20;

const FRAGMENT: &str = "fragment PrFields on PullRequest { id state isDraft title body url \
    author { login __typename } headRefOid mergeStateStatus reviewDecision \
    viewerLatestReview { state commit { oid } } \
    locked viewerCanClose viewerCanUpdateBranch viewerCanEnableAutoMerge \
    autoMergeRequest { enabledAt } \
    labels(first: 20) { nodes { name } } changedFiles createdAt updatedAt \
    commits(last: 1) { nodes { commit { oid statusCheckRollup { state \
    contexts(first: 50) { pageInfo { hasNextPage } nodes { \
    ... on CheckRun { name status conclusion detailsUrl checkSuite { workflowRun { databaseId } } } \
    ... on StatusContext { context state targetUrl } } } } } } } }";

/// Details of the PRs that could be used, plus a note for every one that could not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Details {
    pub items: Vec<PrItem>,
    pub warnings: Vec<String>,
}

impl Client {
    /// Details of `prs`, open or not (the caller checks the `state`). PRs that cannot be read
    /// become warnings. The inbox sources (`notification`, `is_mine`, ...) are left unset.
    pub async fn pr_details(&self, prs: &[PrRef]) -> Result<Details> {
        let all: HashSet<PrRef> = prs.iter().cloned().collect();
        self.details(prs, &all).await
    }

    /// Details of `prs`, fetched in chunks. Merged and closed PRs are kept only when listed in
    /// `keep_closed`.
    pub(crate) async fn details(
        &self,
        prs: &[PrRef],
        keep_closed: &HashSet<PrRef>,
    ) -> Result<Details> {
        let mut details = Details::default();
        for chunk in prs.chunks(CHUNK_SIZE) {
            let query = build_details_query(chunk);
            let data = self.graphql_partial(&query, json!({})).await?;
            let part = parse_details(&data, chunk, keep_closed);
            details.items.extend(part.items);
            details.warnings.extend(part.warnings);
        }
        Ok(details)
    }
}

/// Builds the query for one chunk; alias `p{i}` belongs to `prs[i]`.
fn build_details_query(prs: &[PrRef]) -> String {
    let aliases: String = prs
        .iter()
        .enumerate()
        .map(|(index, pr)| alias(index, pr))
        .collect();
    format!("query {{ {aliases} }} {FRAGMENT}")
}

fn alias(index: usize, pr: &PrRef) -> String {
    let (owner, name) = owner_and_name(&pr.repo);
    format!(
        "p{index}: repository(owner: {}, name: {}) {{ pullRequest(number: {}) {{ ...PrFields }} \
         mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed viewerDefaultMergeMethod \
         autoMergeAllowed isArchived viewerPermission }} ",
        quote(owner),
        quote(name),
        pr.number
    )
}

/// Parses the `data` of a details query for `prs`. Merged and closed PRs are kept (with their
/// `state`) only when listed in `keep_closed`; other ones are dropped.
fn parse_details(data: &Value, prs: &[PrRef], keep_closed: &HashSet<PrRef>) -> Details {
    let mut details = Details::default();
    for (index, pr) in prs.iter().enumerate() {
        let repository = &data[format!("p{index}")];
        let node = &repository["pullRequest"];
        if repository.is_null() || node.is_null() {
            details
                .warnings
                .push(format!("{pr}: not found or no access"));
        } else if let Some(state) = pr_state(node["state"].as_str().unwrap_or_default())
            && (state == PrState::Open || keep_closed.contains(pr))
        {
            details.items.push(parse_item(pr, node, repository, state));
        }
    }
    details
}

/// One pull request. The inbox sources (`notification`, `is_mine`, `review_requested`,
/// `in_scope`) start unset; the inbox fills them in.
fn parse_item(pr: &PrRef, node: &Value, repository: &Value, state: PrState) -> PrItem {
    let (author, is_bot) = parse_author(&node["author"]);
    let text = |key: &str| node[key].as_str().unwrap_or_default().to_owned();
    let (title, head_oid) = (text("title"), text("headRefOid"));
    let review = &node["viewerLatestReview"];
    let viewer_approved = !head_oid.is_empty()
        && review["state"].as_str() == Some("APPROVED")
        && review["commit"]["oid"].as_str() == Some(head_oid.as_str());
    PrItem {
        pr: pr.clone(),
        node_id: text("id"),
        bump: bump_of(&title, node["body"].as_str().unwrap_or_default()),
        title,
        url: text("url"),
        author,
        is_bot,
        draft: node["isDraft"].as_bool().unwrap_or(false),
        checks: parse_checks(node, &head_oid),
        head_oid,
        merge_state: merge_state(node["mergeStateStatus"].as_str().unwrap_or_default()),
        review_decision: review_decision(node["reviewDecision"].as_str()),
        labels: parse_labels(node),
        changed_files: u32::try_from(node["changedFiles"].as_u64().unwrap_or(0)).unwrap_or(0),
        created_at: text("createdAt"),
        updated_at: text("updatedAt"),
        allowed_merge_methods: parse_merge_methods(repository),
        state,
        caps: parse_capabilities(node, repository),
        viewer_approved,
        duplicate_of: None,
        notification: None,
        is_mine: false,
        review_requested: false,
        in_scope: false,
    }
}

/// The viewer's possibilities. Fields GitHub did not return count as allowed.
fn parse_capabilities(node: &Value, repository: &Value) -> Capabilities {
    let flag = |value: &Value| value.as_bool().unwrap_or(true);
    let archived = repository["isArchived"].as_bool().unwrap_or(false);
    let writer = repository["viewerPermission"]
        .as_str()
        .is_none_or(|permission| matches!(permission, "WRITE" | "MAINTAIN" | "ADMIN"));
    let can_merge = writer && !archived;
    let auto_merge = if !node["autoMergeRequest"].is_null() {
        AutoMerge::AlreadyEnabled
    } else if !flag(&repository["autoMergeAllowed"]) {
        AutoMerge::Unavailable("Auto-merge is disabled in this repository's settings".to_owned())
    } else if !flag(&node["viewerCanEnableAutoMerge"]) {
        AutoMerge::Unavailable("GitHub does not let you enable auto-merge here".to_owned())
    } else {
        AutoMerge::Available
    };
    Capabilities {
        archived,
        can_approve: !archived,
        can_merge,
        auto_merge,
        can_update_branch: flag(&node["viewerCanUpdateBranch"]) && !archived,
        can_close: flag(&node["viewerCanClose"]) && !archived,
        can_comment: !archived,
    }
}

/// Returns the login (bots get their `[bot]` suffix) and whether the author is a bot.
fn parse_author(author: &Value) -> (String, bool) {
    let login = author["login"].as_str().unwrap_or("ghost");
    let is_bot = author["__typename"].as_str() == Some("Bot") || login.ends_with("[bot]");
    if is_bot && !login.ends_with("[bot]") {
        (format!("{login}[bot]"), true)
    } else {
        (login.to_owned(), is_bot)
    }
}

fn parse_labels(node: &Value) -> Vec<String> {
    node["labels"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|label| label["name"].as_str().map(str::to_owned))
        .collect()
}

/// The checks of the head commit. Checks beyond the first page and checks of a commit other
/// than the head are summarized by one stand-in from the rollup state, so CI never looks
/// greener than GitHub's own rollup.
fn parse_checks(node: &Value, head_oid: &str) -> Vec<CheckRun> {
    let commit = &node["commits"]["nodes"][0]["commit"];
    let rollup = &commit["statusCheckRollup"];
    if commit["oid"].as_str().is_some_and(|oid| oid != head_oid) {
        return vec![stand_in("checks of an older commit", CheckState::Pending)];
    }
    let mut checks: Vec<CheckRun> = rollup["contexts"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_check)
        .collect();
    if rollup["contexts"]["pageInfo"]["hasNextPage"].as_bool() == Some(true) {
        let state = match rollup["state"].as_str() {
            Some("SUCCESS") => CheckState::Success,
            Some("PENDING" | "EXPECTED") => CheckState::Pending,
            _ => CheckState::Failure,
        };
        checks.push(stand_in("more checks (summary)", state));
    }
    checks
}

fn stand_in(name: &str, state: CheckState) -> CheckRun {
    CheckRun {
        name: name.to_owned(),
        state,
        details_url: None,
        workflow_run_id: None,
    }
}

fn parse_check(context: &Value) -> Option<CheckRun> {
    if let Some(name) = context["context"].as_str() {
        return Some(CheckRun {
            name: name.to_owned(),
            state: status_context_state(context["state"].as_str().unwrap_or_default()),
            details_url: context["targetUrl"].as_str().map(str::to_owned),
            workflow_run_id: None,
        });
    }
    Some(CheckRun {
        name: context["name"].as_str()?.to_owned(),
        state: check_run_state(
            context["status"].as_str().unwrap_or_default(),
            context["conclusion"].as_str(),
        ),
        details_url: context["detailsUrl"].as_str().map(str::to_owned),
        workflow_run_id: context["checkSuite"]["workflowRun"]["databaseId"].as_u64(),
    })
}

fn check_run_state(status: &str, conclusion: Option<&str>) -> CheckState {
    if status != "COMPLETED" {
        return CheckState::Pending;
    }
    match conclusion {
        Some("SUCCESS") => CheckState::Success,
        Some("SKIPPED") => CheckState::Skipped,
        Some("CANCELLED") => CheckState::Cancelled,
        Some("NEUTRAL" | "STALE") => CheckState::Neutral,
        _ => CheckState::Failure,
    }
}

fn status_context_state(state: &str) -> CheckState {
    match state {
        "SUCCESS" => CheckState::Success,
        "PENDING" | "EXPECTED" => CheckState::Pending,
        _ => CheckState::Failure,
    }
}

/// Allowed methods, the viewer's default first.
fn parse_merge_methods(repository: &Value) -> Vec<MergeMethod> {
    let mut methods: Vec<MergeMethod> = [
        ("mergeCommitAllowed", MergeMethod::Merge),
        ("squashMergeAllowed", MergeMethod::Squash),
        ("rebaseMergeAllowed", MergeMethod::Rebase),
    ]
    .into_iter()
    .filter(|(key, _)| repository[key].as_bool().unwrap_or(false))
    .map(|(_, method)| method)
    .collect();
    let preferred = repository["viewerDefaultMergeMethod"]
        .as_str()
        .and_then(merge_method);
    if let Some(position) = preferred.and_then(|m| methods.iter().position(|x| *x == m)) {
        let method = methods.remove(position);
        methods.insert(0, method);
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs() -> Vec<PrRef> {
        (1..=5)
            .map(|number| PrRef {
                repo: "acme/widget".to_owned(),
                number,
            })
            .collect()
    }

    fn parsed() -> Details {
        let data: Value =
            serde_json::from_str(include_str!("../tests/fixtures/pr_details.json")).unwrap();
        parse_details(&data["data"], &refs(), &HashSet::new())
    }

    #[test]
    fn query_has_aliases_and_escapes() {
        let prs = [PrRef {
            repo: "ac\"me/wid".to_owned(),
            number: 9,
        }];
        let query = build_details_query(&prs);
        assert!(query.contains("p0: repository(owner: \"ac\\\"me\", name: \"wid\")"));
        assert!(query.contains("pullRequest(number: 9)"));
        assert!(query.contains("fragment PrFields"));
    }

    #[test]
    fn drops_closed_and_collects_null_warnings() {
        let details = parsed();
        let numbers: Vec<u64> = details.items.iter().map(|i| i.pr.number).collect();
        assert_eq!(numbers, [1, 2, 3]);
        assert_eq!(details.warnings.len(), 1);
        assert!(details.warnings[0].contains("acme/widget#5"));
    }

    #[test]
    fn keeps_non_open_prs_only_with_a_notification() {
        let data: Value =
            serde_json::from_str(include_str!("../tests/fixtures/pr_details.json")).unwrap();
        let mut data = data["data"].clone();
        data["p1"]["pullRequest"]["state"] = Value::from("MERGED");
        let notified = HashSet::from([refs()[1].clone(), refs()[3].clone()]);
        let details = parse_details(&data, &refs(), &notified);
        let found: Vec<(u64, PrState)> = details
            .items
            .iter()
            .map(|item| (item.pr.number, item.state))
            .collect();
        assert_eq!(
            found,
            [
                (1, PrState::Open),
                (2, PrState::Merged),
                (3, PrState::Open),
                (4, PrState::Closed)
            ]
        );
        // The merged one without a notification is dropped, the closed one too.
        let details = parse_details(&data, &refs(), &HashSet::new());
        let numbers: Vec<u64> = details.items.iter().map(|i| i.pr.number).collect();
        assert_eq!(numbers, [1, 3]);
    }

    #[test]
    fn bot_author_is_normalized() {
        let item = &parsed().items[0];
        assert_eq!(item.author, "renovate[bot]");
        assert!(item.is_bot);
        assert_eq!(item.labels, ["dependencies"]);
        assert_eq!(item.changed_files, 2);
        assert_eq!(item.head_oid, "aaaa1111");
        assert_eq!(item.node_id, "PR_kwDO1");
        assert_eq!(item.merge_state, protocol::MergeState::Clean);
    }

    #[test]
    fn mixed_checks() {
        let item = &parsed().items[0];
        let states: Vec<_> = item
            .checks
            .iter()
            .map(|c| (c.name.as_str(), c.state))
            .collect();
        assert_eq!(
            states,
            [
                ("build", CheckState::Success),
                ("lint", CheckState::Failure),
                ("deploy", CheckState::Pending),
                ("ci/legacy", CheckState::Failure),
                ("ci/other", CheckState::Pending),
            ]
        );
        assert_eq!(item.checks[1].workflow_run_id, Some(555));
        assert_eq!(item.checks[3].workflow_run_id, None);
        assert_eq!(
            item.checks[3].details_url.as_deref(),
            Some("https://ci.example.com/1")
        );
    }

    #[test]
    fn merge_methods_default_first() {
        let items = parsed().items;
        assert_eq!(
            items[0].allowed_merge_methods,
            [MergeMethod::Squash, MergeMethod::Merge]
        );
        assert_eq!(items[1].allowed_merge_methods, [MergeMethod::Rebase]);
    }

    #[test]
    fn human_draft_without_checks() {
        let item = &parsed().items[1];
        assert_eq!(item.author, "alice");
        assert!(!item.is_bot);
        assert!(item.draft);
        assert!(item.checks.is_empty());
        assert_eq!(
            item.review_decision,
            protocol::ReviewDecision::ReviewRequired
        );
    }

    #[test]
    fn bump_is_taken_from_title_and_body() {
        let items = parsed().items;
        let serde = items[0].bump.as_ref().unwrap();
        assert_eq!(serde.from.as_deref(), Some("1.0.1"));
        let tokio = items[2].bump.as_ref().unwrap();
        assert_eq!(tokio.from.as_deref(), Some("1.4.2"));
        assert_eq!(tokio.kind, protocol::UpdateKind::Minor);
        assert!(items[1].bump.is_none());
    }

    #[test]
    fn viewer_approval_counts_only_for_the_head() {
        let repository = serde_json::json!({});
        let reviewed = |oid: &str, state: &str| {
            serde_json::json!({"state": "OPEN", "headRefOid": "head1",
                "viewerLatestReview": {"state": state, "commit": {"oid": oid}}})
        };
        let pr = &refs()[0];
        assert!(
            parse_item(
                pr,
                &reviewed("head1", "APPROVED"),
                &repository,
                PrState::Open
            )
            .viewer_approved
        );
        assert!(
            !parse_item(pr, &reviewed("old", "APPROVED"), &repository, PrState::Open)
                .viewer_approved
        );
        assert!(
            !parse_item(
                pr,
                &reviewed("head1", "COMMENTED"),
                &repository,
                PrState::Open
            )
            .viewer_approved
        );
        let unreviewed = serde_json::json!({"state": "OPEN", "headRefOid": "head1"});
        assert!(!parse_item(pr, &unreviewed, &repository, PrState::Open).viewer_approved);
    }

    #[test]
    fn truncated_checks_fall_back_to_the_rollup() {
        let node = |rollup: &str, more: bool| {
            serde_json::json!({"commits": {"nodes": [{"commit": {"oid": "h",
                "statusCheckRollup": {"state": rollup, "contexts": {
                    "pageInfo": {"hasNextPage": more},
                    "nodes": [{"name": "a", "status": "COMPLETED", "conclusion": "SUCCESS"}]}}}}]}})
        };
        let states = |rollup, more| -> Vec<CheckState> {
            parse_checks(&node(rollup, more), "h")
                .iter()
                .map(|c| c.state)
                .collect()
        };
        assert_eq!(states("FAILURE", false), [CheckState::Success]);
        assert_eq!(
            states("FAILURE", true),
            [CheckState::Success, CheckState::Failure]
        );
        assert_eq!(
            states("PENDING", true),
            [CheckState::Success, CheckState::Pending]
        );
        assert_eq!(
            states("SUCCESS", true),
            [CheckState::Success, CheckState::Success]
        );
        let other_commit = parse_checks(&node("SUCCESS", false), "newer");
        assert_eq!(other_commit.len(), 1);
        assert_eq!(other_commit[0].state, CheckState::Pending);
    }

    #[test]
    fn capabilities_come_from_repository_and_pull_request() {
        let items = parsed().items;
        // Fixture 0: write access, auto-merge allowed, nothing special.
        assert_eq!(items[0].caps, Capabilities::default());
        // Fixture 1: read-only viewer in a repository without auto-merge.
        let caps = &items[1].caps;
        assert!(!caps.can_merge);
        assert!(!caps.can_close);
        assert!(!caps.can_update_branch);
        assert!(caps.can_approve);
        assert_eq!(
            caps.auto_merge,
            AutoMerge::Unavailable(
                "Auto-merge is disabled in this repository's settings".to_owned()
            )
        );
        // Fixture 2: auto-merge already enabled.
        assert_eq!(items[2].caps.auto_merge, AutoMerge::AlreadyEnabled);
    }

    #[test]
    fn archived_repository_is_read_only() {
        let node = serde_json::json!({});
        let repository = serde_json::json!({"isArchived": true, "viewerPermission": "ADMIN"});
        let caps = parse_capabilities(&node, &repository);
        assert!(caps.archived && !caps.can_merge && !caps.can_approve && !caps.can_comment);
    }

    #[test]
    fn bot_suffix_detected_by_login() {
        let item = &parsed().items[2];
        assert_eq!(item.author, "dependabot[bot]");
        assert!(item.is_bot);
    }
}
