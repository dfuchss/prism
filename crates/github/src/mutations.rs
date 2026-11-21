//! GraphQL mutations of pull requests, addressed by the node id of the details query.
//! Approving, merging, auto-merge and the branch update are pinned to the head commit that was
//! checked, so GitHub refuses them once new commits arrived.

use crate::client::Client;
use crate::error::{Error, Result};
use crate::mapping::merge_method_name;
use protocol::{MergeMethod, PrItem};
use serde_json::{Value, json};

/// The node id of the pull request; mutations without one are refused.
pub(crate) fn node_id(item: &PrItem) -> Result<&str> {
    if item.node_id.is_empty() {
        return Err(Error::Refused(format!(
            "{}: pull request id unknown (refresh and retry)",
            item.pr
        )));
    }
    Ok(&item.node_id)
}

/// The head commit of the pull request; head-bound mutations without one are refused.
fn head(item: &PrItem) -> Result<&str> {
    if item.head_oid.is_empty() {
        return Err(Error::Refused(format!("{}: head commit unknown", item.pr)));
    }
    Ok(&item.head_oid)
}

impl Client {
    async fn mutate(&self, name: &str, input_type: &str, input: Value) -> Result<()> {
        let query = format!(
            "mutation($input: {input_type}!) {{ {name}(input: $input) {{ clientMutationId }} }}"
        );
        self.graphql(&query, json!({ "input": input }))
            .await
            .map(drop)
    }

    /// A mutation that only applies while the head is still `item.head_oid`: `input` gets the
    /// pull request id and `expectedHeadOid`.
    async fn mutate_head_pinned(
        &self,
        item: &PrItem,
        name: &str,
        input_type: &str,
        mut input: Value,
    ) -> Result<()> {
        input["expectedHeadOid"] = Value::from(head(item)?);
        input["pullRequestId"] = Value::from(node_id(item)?);
        self.mutate(name, input_type, input).await
    }

    /// Approves exactly the head commit, the one that was checked.
    pub(crate) async fn approve(&self, item: &PrItem) -> Result<()> {
        let input = json!({
            "pullRequestId": node_id(item)?,
            "event": "APPROVE",
            "commitOID": head(item)?,
        });
        self.mutate("addPullRequestReview", "AddPullRequestReviewInput", input)
            .await
    }

    /// Merges only while the head is unchanged.
    pub(crate) async fn merge(&self, item: &PrItem, method: MergeMethod) -> Result<()> {
        let input = json!({ "mergeMethod": merge_method_name(method) });
        self.mutate_head_pinned(item, "mergePullRequest", "MergePullRequestInput", input)
            .await
    }

    /// Enables auto-merge only while the head is unchanged.
    pub(crate) async fn enable_auto_merge(&self, item: &PrItem, method: MergeMethod) -> Result<()> {
        let input = json!({ "mergeMethod": merge_method_name(method) });
        self.mutate_head_pinned(
            item,
            "enablePullRequestAutoMerge",
            "EnablePullRequestAutoMergeInput",
            input,
        )
        .await
    }

    /// GitHub's branch update with the rebase method, only while the head is unchanged.
    pub(crate) async fn update_branch(&self, item: &PrItem) -> Result<()> {
        let input = json!({ "updateMethod": "REBASE" });
        self.mutate_head_pinned(
            item,
            "updatePullRequestBranch",
            "UpdatePullRequestBranchInput",
            input,
        )
        .await
    }

    /// Replaces the description of the pull request.
    pub(crate) async fn update_body(&self, item: &PrItem, body: &str) -> Result<()> {
        let input = json!({ "pullRequestId": node_id(item)?, "body": body });
        self.mutate("updatePullRequest", "UpdatePullRequestInput", input)
            .await
    }

    /// Closes the PR, commenting first when a comment is given.
    pub(crate) async fn close(&self, item: &PrItem, comment: Option<&str>) -> Result<()> {
        if let Some(body) = comment {
            self.comment(item, body).await?;
        }
        let input = json!({ "pullRequestId": node_id(item)? });
        self.mutate("closePullRequest", "ClosePullRequestInput", input)
            .await
    }

    pub(crate) async fn comment(&self, item: &PrItem, body: &str) -> Result<()> {
        let input = json!({ "subjectId": node_id(item)?, "body": body });
        self.mutate("addComment", "AddCommentInput", input).await
    }
}
