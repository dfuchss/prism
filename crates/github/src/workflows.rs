//! Workflow runs (REST): the failed jobs of a run and re-running them.

use crate::client::{Client, read_json};
use crate::error::Result;
use protocol::FailedJob;
use reqwest::Method;
use serde_json::Value;

impl Client {
    /// The failed jobs of the latest attempt of a workflow run.
    pub async fn failed_jobs(&self, repo: &str, run_id: u64) -> Result<Vec<FailedJob>> {
        let path = format!("/repos/{repo}/actions/runs/{run_id}/jobs?filter=latest&per_page=100");
        let response = self.rest(Method::GET, &path).await?;
        Ok(parse_failed_jobs(&read_json(response).await?))
    }

    pub(crate) async fn rerun_failed(&self, repo: &str, run_id: u64) -> Result<()> {
        let path = format!("/repos/{repo}/actions/runs/{run_id}/rerun-failed-jobs");
        self.rest(Method::POST, &path).await.map(drop)
    }
}

/// Failed jobs (`conclusion == failure`) of a `.../actions/runs/{id}/jobs` response.
fn parse_failed_jobs(response: &Value) -> Vec<FailedJob> {
    response["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|job| job["conclusion"].as_str() == Some("failure"))
        .filter_map(|job| {
            Some(FailedJob {
                name: job["name"].as_str()?.to_owned(),
                html_url: job["html_url"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_failed_jobs() {
        let response: Value =
            serde_json::from_str(include_str!("../tests/fixtures/jobs.json")).unwrap();
        let jobs = parse_failed_jobs(&response);
        assert_eq!(
            jobs,
            [FailedJob {
                name: "lint".to_owned(),
                html_url: "https://github.com/acme/widget/actions/runs/555/job/2".to_owned()
            }]
        );
    }
}
