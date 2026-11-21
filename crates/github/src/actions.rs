//! `perform`: runs a protocol [`Action`] against a PR.

use crate::client::Client;
use crate::error::{Error, Result};
use protocol::{Action, ActionResult, CheckState, MergeMethod, PrItem};
use std::collections::HashSet;

impl Client {
    /// Runs the action; failures become `ok == false` with the error as message.
    pub async fn perform(&self, item: &PrItem, action: &Action) -> ActionResult {
        let outcome = self.run(item, action).await;
        ActionResult {
            pr: item.pr.clone(),
            action: action.clone(),
            ok: outcome.is_ok(),
            message: outcome.unwrap_or_else(|error| error.to_string()),
        }
    }

    async fn run(&self, item: &PrItem, action: &Action) -> Result<String> {
        match action {
            Action::Approve => self.approve_item(item).await,
            Action::Merge(method) => self.merge_item(item, *method).await,
            Action::ApproveAndMerge(method) => {
                self.approve_item(item).await?;
                self.merge_item(item, *method).await
            }
            Action::EnableAutoMerge(method) => {
                let method = choose_method(item, *method)?;
                self.enable_auto_merge(item, method).await?;
                Ok("auto-merge enabled".to_owned())
            }
            Action::Close { comment } => {
                self.close(item, comment.as_deref()).await?;
                Ok("closed".to_owned())
            }
            Action::Comment(body) => {
                self.comment(item, body).await?;
                Ok("commented".to_owned())
            }
            Action::RerunFailed => self.rerun_item(item).await,
            Action::Rebase => self.rebase_item(item).await,
            Action::MarkRead | Action::MarkDone | Action::Unsubscribe => {
                self.notification_action(item, action).await
            }
        }
    }

    /// Approves the head commit, unless the viewer approved it already.
    async fn approve_item(&self, item: &PrItem) -> Result<String> {
        if item.viewer_approved {
            return Ok("already approved".to_owned());
        }
        self.approve(item).await?;
        Ok("approved".to_owned())
    }

    async fn merge_item(&self, item: &PrItem, method: Option<MergeMethod>) -> Result<String> {
        let method = choose_method(item, method)?;
        self.merge(item, method).await?;
        Ok("merged".to_owned())
    }

    async fn notification_action(&self, item: &PrItem, action: &Action) -> Result<String> {
        let thread = &item
            .notification
            .as_ref()
            .ok_or_else(|| Error::Refused("no notification thread for this PR".to_owned()))?
            .thread_id;
        match action {
            Action::MarkRead => self.mark_read(thread).await.map(|()| "marked read"),
            Action::MarkDone => self.mark_done(thread).await.map(|()| "marked done"),
            Action::Unsubscribe => self.unsubscribe(thread).await.map(|()| "unsubscribed"),
            other => Err(Error::Refused(format!(
                "{other} is not a notification action"
            ))),
        }
        .map(str::to_owned)
    }

    async fn rerun_item(&self, item: &PrItem) -> Result<String> {
        let runs = failing_run_ids(item);
        if runs.is_empty() {
            return Err(Error::Refused(
                "no failing workflow run to rerun".to_owned(),
            ));
        }
        for run in &runs {
            self.rerun_failed(&item.pr.repo, *run).await?;
        }
        Ok(format!(
            "re-running failed jobs of {} workflow run(s)",
            runs.len()
        ))
    }
}

/// The requested method (which must be allowed) or the repository's first allowed one.
fn choose_method(item: &PrItem, requested: Option<MergeMethod>) -> Result<MergeMethod> {
    let allowed = &item.allowed_merge_methods;
    match requested {
        Some(method) if allowed.contains(&method) => Ok(method),
        Some(method) => Err(Error::Refused(format!(
            "merge method {method:?} is not allowed in this repository"
        ))),
        None => allowed
            .first()
            .copied()
            .ok_or_else(|| Error::Refused("the repository allows no merge method".to_owned())),
    }
}

/// Distinct workflow run ids of the failing checks, in order.
fn failing_run_ids(item: &PrItem) -> Vec<u64> {
    let mut seen = HashSet::new();
    item.checks
        .iter()
        .filter(|check| check.state == CheckState::Failure)
        .filter_map(|check| check.workflow_run_id)
        .filter(|id| seen.insert(*id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::CheckRun;

    fn sample_item() -> PrItem {
        PrItem::sample("acme/widget", 1, "t")
    }

    #[test]
    fn method_choice() {
        let mut item = sample_item();
        item.allowed_merge_methods = vec![MergeMethod::Squash, MergeMethod::Merge];
        assert_eq!(choose_method(&item, None).unwrap(), MergeMethod::Squash);
        assert_eq!(
            choose_method(&item, Some(MergeMethod::Merge)).unwrap(),
            MergeMethod::Merge
        );
        assert!(choose_method(&item, Some(MergeMethod::Rebase)).is_err());
        item.allowed_merge_methods.clear();
        assert!(choose_method(&item, None).is_err());
    }

    #[test]
    fn distinct_failing_runs() {
        let check = |state, run| CheckRun {
            name: "c".to_owned(),
            state,
            details_url: None,
            workflow_run_id: run,
        };
        let mut item = sample_item();
        item.checks = vec![
            check(CheckState::Failure, Some(5)),
            check(CheckState::Failure, Some(5)),
            check(CheckState::Success, Some(6)),
            check(CheckState::Failure, None),
            check(CheckState::Failure, Some(7)),
        ];
        assert_eq!(failing_run_ids(&item), [5, 7]);
    }
}
