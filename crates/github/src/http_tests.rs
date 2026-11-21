//! HTTP-level tests against a local mock server.

use crate::token::TokenSource;
use crate::{Client, Error};
use protocol::{Action, MergeMethod, PrItem};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{
    body_partial_json, body_string_contains, header, method, path, query_param,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer, token: &'static str) -> Client {
    let tokens = TokenSource::with_provider(move || Ok(token.to_owned()));
    Client::with_base_url(&server.uri(), tokens)
}

/// A human PR with node id `PR_node` and head `aaaa1111` in a rebase/merge repository.
fn item() -> PrItem {
    let mut item = PrItem::sample("acme/widget", 1, "t");
    item.author = "alice".to_owned();
    item.is_bot = false;
    item.head_oid = "aaaa1111".to_owned();
    item.allowed_merge_methods = vec![MergeMethod::Rebase, MergeMethod::Merge];
    item
}

/// Answers the mutation `name` with success.
fn mutation_ok(name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"data": {name: {"clientMutationId": null}}}))
}

#[tokio::test]
async fn refetches_token_and_retries_after_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer old"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer new"))
        .and(header("user-agent", "prism"))
        .and(header("x-github-api-version", "2022-11-28"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"viewer": {"login": "alice"}}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let tokens = TokenSource::with_provider(move || {
        Ok(if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            "old"
        } else {
            "new"
        }
        .to_owned())
    });
    let client = Client::with_base_url(&server.uri(), tokens);
    assert_eq!(client.viewer_login().await.unwrap(), "alice");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    // cached afterwards: no further request (expect(1) above would fail on drop)
    assert_eq!(client.viewer_login().await.unwrap(), "alice");
}

#[tokio::test]
async fn persistent_401_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .expect(2)
        .mount(&server)
        .await;
    let error = client(&server, "bad").viewer_login().await.unwrap_err();
    assert!(matches!(error, Error::Unauthorized));
}

#[tokio::test]
async fn merge_sends_expected_head_oid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("mergePullRequest"))
        .and(body_partial_json(json!({"variables": {"input": {
            "pullRequestId": "PR_node",
            "mergeMethod": "REBASE",
            "expectedHeadOid": "aaaa1111",
        }}})))
        .respond_with(mutation_ok("mergePullRequest"))
        .expect(1)
        .mount(&server)
        .await;
    let result = client(&server, "t")
        .perform(&item(), &Action::Merge(None))
        .await;
    assert!(result.ok, "{}", result.message);
}

#[tokio::test]
async fn rejected_merge_reports_failure() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("mergePullRequest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": null, "errors": [{"message": "Head branch was modified"}]}),
        ))
        .mount(&server)
        .await;
    let result = client(&server, "t")
        .perform(&item(), &Action::Merge(None))
        .await;
    assert!(!result.ok);
    assert!(result.message.contains("Head branch was modified"));
}

#[tokio::test]
async fn notifications_follow_next_link_and_read_the_poll_interval() {
    let server = MockServer::start().await;
    let page1 = json!([{"id": "1", "unread": true, "reason": "author", "updated_at": "t1",
        "subject": {"type": "PullRequest", "url": "https://api.github.com/repos/acme/widget/pulls/1"}}]);
    let page2 = json!([{"id": "2", "unread": false, "reason": "mention", "updated_at": "t2",
        "subject": {"type": "Issue", "url": "https://api.github.com/repos/acme/widget/issues/2"}},
        {"id": "3", "unread": false, "reason": "mention", "updated_at": "t3",
        "subject": {"type": "PullRequest", "url": "https://api.github.com/repos/acme/widget/pulls/3"}}]);
    let next = format!("<{}/notifications?page=2>; rel=\"next\"", server.uri());
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page2))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .and(query_param("per_page", "50"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .insert_header("x-poll-interval", "90")
                .set_body_json(page1),
        )
        .mount(&server)
        .await;
    let client = client(&server, "t");
    let threads = client.notifications(true).await.unwrap();
    let ids: Vec<&str> = threads.iter().map(|t| t.info.thread_id.as_str()).collect();
    assert_eq!(ids, ["1", "3"]);
    assert_eq!(client.poll_interval_secs(), Some(90));
}

#[tokio::test]
async fn links_to_other_hosts_are_not_followed() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    let next = format!("<{}/notifications?page=2>; rel=\"next\"", elsewhere.uri());
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .set_body_json(json!([])),
        )
        .mount(&server)
        .await;
    let error = client(&server, "t").notifications(true).await.unwrap_err();
    assert!(matches!(error, Error::Parse(_)), "{error}");
    assert!(elsewhere.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn rate_limits_are_reported_and_delay_the_next_poll() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    let client = client(&server, "t");
    let error = client.viewer_login().await.unwrap_err();
    assert!(
        matches!(
            error,
            Error::RateLimited {
                retry_after_secs: 120
            }
        ),
        "{error}"
    );
    assert!(client.poll_interval_secs().is_some_and(|secs| secs >= 119));
}

#[tokio::test]
async fn errors_carry_githubs_message_and_bad_json_is_a_parse_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(422).set_body_json(
            json!({"message": "Validation Failed", "documentation_url": "https://docs.github.com"}),
        ))
        .mount(&server)
        .await;
    let client = client(&server, "t");
    let error = client.viewer_login().await.unwrap_err();
    assert!(matches!(error, Error::Parse(_)), "{error}");
    let error = client.failed_jobs("acme/widget", 1).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "GitHub returned HTTP 422: Validation Failed"
    );
}

#[tokio::test]
async fn notification_actions_need_a_thread() {
    let server = MockServer::start().await;
    let result = client(&server, "t")
        .perform(&item(), &Action::MarkDone)
        .await;
    assert!(!result.ok);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn approve_pins_the_head_and_skips_when_already_approved() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("addPullRequestReview"))
        .and(body_partial_json(json!({"variables": {"input": {
            "pullRequestId": "PR_node", "event": "APPROVE", "commitOID": "aaaa1111",
        }}})))
        .respond_with(mutation_ok("addPullRequestReview"))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server, "t");
    let result = client.perform(&item(), &Action::Approve).await;
    assert!(result.ok, "{}", result.message);
    let mut approved = item();
    approved.viewer_approved = true;
    let again = client.perform(&approved, &Action::Approve).await;
    assert!(again.ok);
    assert_eq!(again.message, "already approved");
}

#[tokio::test]
async fn failed_approve_never_merges() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("addPullRequestReview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": null, "errors": [{"message": "Can not approve your own pull request"}]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("mergePullRequest"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let result = client(&server, "t")
        .perform(&item(), &Action::ApproveAndMerge(None))
        .await;
    assert!(!result.ok);
}

#[tokio::test]
async fn auto_merge_sends_expected_head_and_empty_head_is_refused() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("enablePullRequestAutoMerge"))
        .and(body_partial_json(json!({"variables": {"input": {
            "expectedHeadOid": "aaaa1111",
        }}})))
        .respond_with(mutation_ok("enablePullRequestAutoMerge"))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server, "t");
    let result = client
        .perform(&item(), &Action::EnableAutoMerge(None))
        .await;
    assert!(result.ok, "{}", result.message);
    let mut headless = item();
    headless.head_oid.clear();
    for action in [Action::Merge(None), Action::Approve] {
        let refused = client.perform(&headless, &action).await;
        assert!(!refused.ok);
    }
}

#[tokio::test]
async fn search_tolerates_an_inaccessible_result() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("search(query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"search": {"pageInfo": {"hasNextPage": false, "endCursor": null},
                "nodes": [null, {"number": 7, "repository": {"nameWithOwner": "acme/widget"}}]}},
            "errors": [{"message": "Resource protected by organization SAML enforcement"}],
        })))
        .mount(&server)
        .await;
    let hits = client(&server, "t").search_open_prs().await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].pr.number, 7);
}

#[tokio::test]
async fn lists_repos_and_open_prs_with_paging() {
    let server = MockServer::start().await;
    let repos_page = |names: &[&str], next: Option<&str>| {
        json!({"data": {"viewer": {"repositories": {
            "nodes": names.iter().map(|n| json!({"nameWithOwner": n})).collect::<Vec<_>>(),
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}}}}})
    };
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"variables": {"after": null}})))
        .and(body_string_contains("viewer"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(repos_page(&["acme/widget"], Some("P2"))),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"variables": {"after": "P2"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(repos_page(&["acme/gadget"], None)))
        .mount(&server)
        .await;
    let repos = client(&server, "t").accessible_repos().await.unwrap();
    assert_eq!(repos, ["acme/widget", "acme/gadget"]);

    // First request: widget has a second page; the follow-up asks for it with the cursor.
    Mock::given(method("POST"))
        .and(body_string_contains(r#"after: \"R2\""#))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"r0": {
            "pullRequests": {"nodes": [{"number": 8}],
                "pageInfo": {"hasNextPage": false, "endCursor": null}}}}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("r1: repository"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "r0": {"pullRequests": {"nodes": [{"number": 7}],
                "pageInfo": {"hasNextPage": true, "endCursor": "R2"}}},
            "r1": {"pullRequests": {"nodes": [{"number": 1}],
                "pageInfo": {"hasNextPage": false, "endCursor": null}}}}})))
        .expect(1)
        .mount(&server)
        .await;
    let prs = client(&server, "t").open_pr_refs(&repos).await.unwrap();
    let found: Vec<_> = prs.iter().map(|p| (p.repo.as_str(), p.number)).collect();
    assert_eq!(
        found,
        [("acme/widget", 7), ("acme/gadget", 1), ("acme/widget", 8)]
    );
}

#[tokio::test]
async fn rebase_dependabot_comments_the_command() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("addComment"))
        .and(body_partial_json(json!({"variables": {"input": {
            "subjectId": "PR_node", "body": "@dependabot rebase",
        }}})))
        .respond_with(mutation_ok("addComment"))
        .expect(1)
        .mount(&server)
        .await;
    let mut bot = item();
    bot.author = "dependabot[bot]".to_owned();
    let result = client(&server, "t").perform(&bot, &Action::Rebase).await;
    assert!(result.ok, "{}", result.message);
}

fn renovate_body_mock(body: &str) -> Mock {
    Mock::given(method("POST"))
        .and(body_string_contains("node(id: $id)"))
        .and(body_partial_json(json!({"variables": {"id": "PR_node"}})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"node": {"body": body}}})),
        )
}

#[tokio::test]
async fn rebase_renovate_ticks_the_checkbox() {
    let server = MockServer::start().await;
    renovate_body_mock("Bump\n\n- [ ] <!-- rebase-check -->rebase me\n- [ ] <!-- other -->x")
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("updatePullRequest"))
        .and(body_partial_json(json!({"variables": {"input": {
            "pullRequestId": "PR_node",
            "body": "Bump\n\n- [x] <!-- rebase-check -->rebase me\n- [ ] <!-- other -->x",
        }}})))
        .respond_with(mutation_ok("updatePullRequest"))
        .expect(1)
        .mount(&server)
        .await;
    let mut bot = item();
    bot.author = "renovate[bot]".to_owned();
    let result = client(&server, "t").perform(&bot, &Action::Rebase).await;
    assert!(result.ok, "{}", result.message);
}

#[tokio::test]
async fn rebase_renovate_refuses_missing_or_checked_box() {
    for (body, message) in [
        ("no box here", "no rebase checkbox"),
        ("- [x] <!-- rebase-check -->x", "rebase already requested"),
    ] {
        let server = MockServer::start().await;
        renovate_body_mock(body).mount(&server).await;
        Mock::given(method("POST"))
            .and(body_string_contains("updatePullRequest"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let mut bot = item();
        bot.author = "renovate[bot]".to_owned();
        let result = client(&server, "t").perform(&bot, &Action::Rebase).await;
        assert!(!result.ok);
        assert!(result.message.contains(message), "{}", result.message);
    }
}

#[tokio::test]
async fn rebase_of_other_authors_updates_the_branch_with_the_head() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("updatePullRequestBranch"))
        .and(body_partial_json(json!({"variables": {"input": {
            "pullRequestId": "PR_node", "expectedHeadOid": "aaaa1111", "updateMethod": "REBASE",
        }}})))
        .respond_with(mutation_ok("updatePullRequestBranch"))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server, "t");
    let result = client.perform(&item(), &Action::Rebase).await;
    assert!(result.ok, "{}", result.message);
    let mut headless = item();
    headless.head_oid.clear();
    assert!(!client.perform(&headless, &Action::Rebase).await.ok);
}
