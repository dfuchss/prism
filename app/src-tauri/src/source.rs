//! Where the inbox comes from and where actions go: GitHub (the [`Client`]), or the local demo
//! data of [`crate::demo`] (`PRISM_DEMO=1`, for screenshots). The rest of the backend, including
//! every safety check of `exec.rs`, is the same for both.

use crate::demo::DemoSource;
use prism_github::{Client, Details, Result};
use protocol::{Action, ActionResult, FailedJob, InboxSnapshot, PrItem, PrRef};

pub(crate) enum Source {
    GitHub(Client),
    Demo(DemoSource),
}

impl Source {
    pub(crate) async fn inbox(&self, scope: &[PrRef]) -> Result<InboxSnapshot> {
        match self {
            Self::GitHub(client) => client.inbox(scope).await,
            Self::Demo(demo) => Ok(demo.inbox(scope)),
        }
    }

    pub(crate) async fn pr_details(&self, prs: &[PrRef]) -> Result<Details> {
        match self {
            Self::GitHub(client) => client.pr_details(prs).await,
            Self::Demo(demo) => Ok(demo.pr_details(prs)),
        }
    }

    pub(crate) async fn perform(&self, item: &PrItem, action: &Action) -> ActionResult {
        match self {
            Self::GitHub(client) => client.perform(item, action).await,
            Self::Demo(demo) => demo.perform(item, action).await,
        }
    }

    pub(crate) async fn failed_jobs(&self, repo: &str, run_id: u64) -> Result<Vec<FailedJob>> {
        match self {
            Self::GitHub(client) => client.failed_jobs(repo, run_id).await,
            Self::Demo(demo) => Ok(demo.failed_jobs(repo, run_id)),
        }
    }

    pub(crate) async fn accessible_repos(&self) -> Result<Vec<String>> {
        match self {
            Self::GitHub(client) => client.accessible_repos().await,
            Self::Demo(demo) => Ok(demo.accessible_repos()),
        }
    }

    pub(crate) async fn open_pr_refs(&self, repos: &[String]) -> Result<Vec<PrRef>> {
        match self {
            Self::GitHub(client) => client.open_pr_refs(repos).await,
            Self::Demo(demo) => Ok(demo.open_pr_refs(repos)),
        }
    }

    pub(crate) async fn viewer_login(&self) -> Result<String> {
        match self {
            Self::GitHub(client) => client.viewer_login().await,
            Self::Demo(_) => Ok(crate::demo::VIEWER.to_owned()),
        }
    }

    /// Where the token comes from; `demo` for the demo data.
    pub(crate) fn auth_source(&self) -> String {
        match self {
            Self::GitHub(client) => client.auth_source(),
            Self::Demo(_) => "demo".to_owned(),
        }
    }

    pub(crate) fn set_gh_path(&self, path: &str) {
        if let Self::GitHub(client) = self {
            client.set_gh_path(path);
        }
    }

    pub(crate) fn poll_interval_secs(&self) -> Option<u64> {
        match self {
            Self::GitHub(client) => client.poll_interval_secs(),
            Self::Demo(_) => None,
        }
    }

    /// The screen the demo UI starts on (`PRISM_DEMO_VIEW`); `None` outside demo mode.
    pub(crate) fn demo_view(&self) -> Option<String> {
        match self {
            Self::GitHub(_) => None,
            Self::Demo(demo) => demo.view().map(str::to_owned),
        }
    }
}
