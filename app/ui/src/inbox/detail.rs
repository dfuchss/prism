//! Right column: details of the open pull request and its actions.
//!
//! The pane exists per open PR. Its parts follow a memo of the item, so a refresh that leaves
//! the PR unchanged re-renders nothing, and the failed jobs are fetched once per failing head
//! commit, not on every refresh.

use crate::api;
use crate::format::{check_class, check_label, merge_state_label, review_label};
use crate::state::Ui;
use crate::widgets::{ActionButtons, Loader};
use leptos::prelude::*;
use protocol::{CheckRun, CheckState, FailedJob, PrItem, PrRef};

#[component]
pub(crate) fn Detail(pr: PrRef) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let item = ui.item(pr.clone());
    // The head commit while a check fails: the failed jobs belong to it.
    let failing = Memo::new(move |_| {
        item.with(|item| {
            item.as_ref()
                .filter(|item| item.checks.iter().any(|c| c.state == CheckState::Failure))
                .map(|item| item.head_oid.clone())
        })
    });
    let targets = Signal::stored(vec![pr.clone()]);
    view! {
        <aside class="detail">
            <div class="detail-head">
                <button class="ghost" on:click=move |_| ui.open.set(None)>"Close pane"</button>
            </div>
            {move || item.get().map(|item| view! { <Body item/> })}
            {move || failing.get().map(|_| view! { <FailedJobs pr=pr.clone()/> })}
            <h4>"Actions"</h4>
            <div class="buttons">
                <ActionButtons targets/>
            </div>
        </aside>
    }
}

#[component]
fn Body(item: PrItem) -> impl IntoView {
    let url = item.url.clone();
    view! {
        <h3>
            <a href=url.clone() on:click=move |ev| { ev.prevent_default(); api::open_url(&url) }>
                {item.title.clone()}
            </a>
        </h3>
        <p class="muted">{format!("{} by {}", item.pr, item.author)}</p>
        {facts(&item)}
        {checks(&item.checks)}
    }
}

fn facts(item: &PrItem) -> impl IntoView + use<> {
    let labels = if item.labels.is_empty() {
        view! { <span class="muted">"none"</span> }.into_any()
    } else {
        item.labels
            .iter()
            .map(|l| view! { <span class="badge">{l.clone()}</span> })
            .collect_view()
            .into_any()
    };
    view! {
        <dl class="facts">
            <dt>"Merge state"</dt>
            <dd>{merge_state_label(item.merge_state)}</dd>
            <dt>"Review"</dt>
            <dd>{review_label(item.review_decision)}</dd>
            <dt>"Files"</dt>
            <dd>{item.changed_files}</dd>
            <dt>"Labels"</dt>
            <dd>{labels}</dd>
        </dl>
    }
}

fn check_line(check: &CheckRun) -> impl IntoView + use<> {
    let link = check.details_url.clone();
    view! {
        <li>
            <span class=format!("dot {}", check_class(check.state))></span>
            <span class="grow">{check.name.clone()}</span>
            <span class="muted">{check_label(check.state)}</span>
            {link.map(|url| view! { <a href=url.clone() on:click=move |ev| { ev.prevent_default(); api::open_url(&url) }>"open"</a> })}
        </li>
    }
}

fn checks(checks: &[CheckRun]) -> impl IntoView + use<> {
    view! {
        <h4>"Checks"</h4>
        <ul class="checks">{checks.iter().map(check_line).collect_view()}</ul>
        {checks.is_empty().then(|| view! { <p class="muted">"No checks."</p> })}
    }
}

/// Loads the failed jobs of the PR's failing workflow runs (once per mount: the parent mounts
/// it again when the failing head commit changes).
#[component]
fn FailedJobs(pr: PrRef) -> impl IntoView {
    let jobs = RwSignal::new(None::<Result<Vec<FailedJob>, String>>);
    Loader::new().fetch(async move { api::failed_jobs(&pr).await }, move |result| {
        jobs.set(Some(result));
    });
    view! {
        <h4>"Failed jobs"</h4>
        {move || match jobs.get() {
            None => view! { <p class="muted">"loading..."</p> }.into_any(),
            Some(Err(error)) => view! { <p class="result bad">{error}</p> }.into_any(),
            Some(Ok(list)) if list.is_empty() => view! { <p class="muted">"none found"</p> }.into_any(),
            Some(Ok(list)) => view! { <ul class="checks">{list.into_iter().map(job_line).collect_view()}</ul> }.into_any(),
        }}
    }
}

fn job_line(job: FailedJob) -> impl IntoView {
    let url = job.html_url.clone();
    view! {
        <li>
            <span class="dot ci-red"></span>
            <span class="grow">{job.name}</span>
            <a href=url.clone() on:click=move |ev| { ev.prevent_default(); api::open_url(&url) }>"log"</a>
        </li>
    }
}
