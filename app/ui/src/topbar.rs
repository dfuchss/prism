//! Top bar: tabs, refresh and the error banner.

use crate::api;
use crate::format::{secs_since, updated};
use crate::state::{Activity, Clock, Tab, Ui};
use leptos::prelude::*;

#[component]
pub(crate) fn TopBar() -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {
        <header class="topbar">
            <span class="brand">
                <img src="favicon.svg" alt="" width="18" height="18"/>
                "PRism"
            </span>
            <nav class="tabs">
                {Tab::MAIN.into_iter().map(|tab| view! { <TabButton tab/> }).collect_view()}
            </nav>
            <span class="spacer"></span>
            <Updated/>
            <button
                class="primary"
                disabled=move || ui.activity.get() != Activity::Idle
                on:click=move |_| refresh(ui)
            >
                <span
                    class="spin reserve"
                    class:on=move || ui.refreshing.get() || ui.activity.get() == Activity::Refreshing
                ></span>
                "Refresh"
            </button>
            <TabButton tab=Tab::About/>
        </header>
        <ErrorBanner/>
    }
}

#[component]
fn TabButton(tab: Tab) -> impl IntoView {
    let ui = expect_context::<Ui>();
    view! {
        <button
            class="tab"
            class:active=move || ui.tab.get() == tab
            on:click=move |_| ui.tab.set(tab)
        >
            {tab.label()}
        </button>
    }
}

/// The `inbox-updated` event usually delivers the snapshot first; `set_snapshot` ignores the
/// same one coming back as the answer.
fn refresh(ui: Ui) {
    ui.activity.set(Activity::Refreshing);
    leptos::task::spawn_local(async move {
        match api::refresh().await {
            Ok(snapshot) => ui.set_snapshot(snapshot),
            Err(error) => ui.notice.set(Some(error)),
        }
        ui.activity.set(Activity::Idle);
    });
}

#[component]
fn Updated() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let clock = expect_context::<Clock>();
    let text = move || {
        let now = clock.0.get();
        ui.snapshot.with(|snapshot| match snapshot {
            None => "loading...".to_owned(),
            Some(snapshot) => {
                secs_since(&snapshot.fetched_at, now).map_or_else(String::new, updated)
            }
        })
    };
    view! { <span class="muted updated">{text}</span> }
}

#[component]
fn ErrorBanner() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let error = Memo::new(move |_| {
        ui.snapshot
            .with(|snapshot| snapshot.as_ref().and_then(|s| s.error.clone()))
    });
    view! {
        {move || {
            error
                .get()
                .map(|message| {
                    let auth = message.contains("gh auth login");
                    view! {
                        <div class="banner">
                            {auth
                                .then(|| {
                                    view! {
                                        <strong>"GitHub is not signed in. "</strong>
                                        "Run "
                                        <code>"gh auth login"</code>
                                        " in a terminal, then press Refresh. "
                                    }
                                })}
                            <span class="muted">{message}</span>
                        </div>
                    }
                })
        }}
    }
}
