//! The Settings tab: edit `settings.toml` and preview which repositories its scope covers.

use crate::api;
use crate::state::Editors;
use crate::widgets::{Doc, Loader, TomlEditor};
use leptos::prelude::*;
use protocol::ScopePreview;

const HELP: &str = "scope.include / scope.exclude: globs over owner/name (\"owner/*\", \
\"*/prism\", \"owner/repo\"). include adds the open PRs of the matching repositories you can \
access to the inbox (marked \"via repo\"); empty include: nothing is added. exclude hides the \
matching repositories from everything (notifications, review requests and your own PRs too; \
rules never see them) and wins over include.";

type Preview = Option<Result<ScopePreview, String>>;

#[component]
pub(crate) fn Settings() -> impl IntoView {
    let editors = expect_context::<Editors>();
    let preview = RwSignal::<Preview>::new(None);
    let loader = Loader::new();
    // Previews the text on screen, saved or not.
    let load_preview = move || {
        preview.set(None);
        let toml = editors.settings.text.get_untracked();
        loader.fetch(
            async move { api::scope_preview(&toml).await },
            move |result| {
                preview.set(Some(result));
            },
        );
    };
    view! {
        <TomlEditor
            doc=Doc::Settings
            short=true
            on_ready=move || load_preview()
            extra=Box::new(move || {
                view! { <button on:click=move |_| load_preview()>"Preview scope"</button> }
                    .into_any()
            })
        >
            <p class="muted help">{HELP}</p>
            <RepoList preview/>
        </TomlEditor>
    }
}

#[component]
fn RepoList(preview: RwSignal<Preview>) -> impl IntoView {
    view! {
        <div class="repo-list">
            {move || match preview.get() {
                None => view! { <p class="muted">"Loading repositories..."</p> }.into_any(),
                Some(Err(error)) => view! { <pre class="result bad">{error}</pre> }.into_any(),
                Some(Ok(preview)) => {
                    view! {
                        <Repos title="in scope" repos=preview.covered/>
                        <Repos title="excluded" repos=preview.excluded/>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn Repos(title: &'static str, repos: Vec<String>) -> impl IntoView {
    if repos.is_empty() {
        return view! { <p class="muted">{format!("No accessible repository is {title}.")}</p> }
            .into_any();
    }
    view! {
        <h4>{format!("{} repositories {title}", repos.len())}</h4>
        <ul class="mono">{repos.into_iter().map(|repo| view! { <li>{repo}</li> }).collect_view()}</ul>
    }
        .into_any()
}
