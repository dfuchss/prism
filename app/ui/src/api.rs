//! Typed wrappers of the backend commands.

use crate::tauri::call;
use protocol::{
    ActTarget, Action, ActionResult, AppInfo, AuditEntry, FailedJob, InboxSnapshot, PlannedAction,
    PrRef, ScopePreview,
};
use serde::Serialize;

#[derive(Serialize)]
struct ActArgs<'a> {
    targets: Vec<ActTarget>,
    action: &'a Action,
}

#[derive(Serialize)]
struct PlannedArgs<'a> {
    planned: &'a [PlannedAction],
}

#[derive(Serialize)]
struct PrArgs<'a> {
    pr: &'a PrRef,
}

#[derive(Serialize)]
struct TomlArgs<'a> {
    toml: &'a str,
}

#[derive(Serialize)]
struct LimitArgs {
    limit: usize,
}

#[derive(Serialize)]
struct UrlArgs<'a> {
    url: &'a str,
}

pub(crate) async fn get_inbox() -> Result<Option<InboxSnapshot>, String> {
    call("get_inbox", ()).await
}

pub(crate) async fn refresh() -> Result<InboxSnapshot, String> {
    call("refresh", ()).await
}

pub(crate) async fn act(
    prs: &[PrRef],
    heads: &[String],
    action: &Action,
) -> Result<Vec<ActionResult>, String> {
    // `heads` are the head commits the user saw, parallel to `prs`.
    let targets = prs
        .iter()
        .enumerate()
        .map(|(index, pr)| ActTarget {
            pr: pr.clone(),
            head: heads.get(index).cloned(),
        })
        .collect();
    call("act", ActArgs { targets, action }).await
}

pub(crate) async fn apply_planned(planned: &[PlannedAction]) -> Result<Vec<ActionResult>, String> {
    call("apply_planned", PlannedArgs { planned }).await
}

pub(crate) async fn planned_actions() -> Result<Vec<PlannedAction>, String> {
    call("planned_actions", ()).await
}

pub(crate) async fn failed_jobs(pr: &PrRef) -> Result<Vec<FailedJob>, String> {
    call("failed_jobs", PrArgs { pr }).await
}

pub(crate) async fn get_rules() -> Result<String, String> {
    call("get_rules", ()).await
}

pub(crate) async fn save_rules(toml: &str) -> Result<Vec<String>, String> {
    call("save_rules", TomlArgs { toml }).await
}

pub(crate) async fn get_settings() -> Result<String, String> {
    call("get_settings", ()).await
}

pub(crate) async fn save_settings(toml: &str) -> Result<Vec<String>, String> {
    call("save_settings", TomlArgs { toml }).await
}

/// The accessible repositories the scope of the (unsaved) `toml` covers.
pub(crate) async fn scope_preview(toml: &str) -> Result<ScopePreview, String> {
    call("scope_preview", TomlArgs { toml }).await
}

pub(crate) async fn audit_log(limit: usize) -> Result<Vec<AuditEntry>, String> {
    call("audit_log", LimitArgs { limit }).await
}

/// Opens a GitHub URL in the browser; failures are only logged.
pub(crate) fn open_url(url: &str) {
    let url = url.to_owned();
    leptos::task::spawn_local(async move {
        if let Err(error) = call::<()>("open_url", UrlArgs { url: &url }).await {
            web_sys::console::error_1(&error.into());
        }
    });
}

pub(crate) async fn app_info() -> Result<AppInfo, String> {
    call("app_info", ()).await
}

/// The screen to start on in demo mode; `None` normally.
pub(crate) async fn demo_view() -> Result<Option<String>, String> {
    call("demo_view", ()).await
}
