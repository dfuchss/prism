//! Prism UI (Leptos, CSR, built with Trunk).

// Leptos' `#[component]` generates `pub` props structs inside restricted modules.
#![allow(unreachable_pub)]
// Component props are taken by value, as `#[component]` requires.
#![allow(clippy::needless_pass_by_value)]

mod about;
mod actions;
mod api;
mod audit;
mod confirm;
mod demo;
mod filter;
mod format;
mod inbox;
mod planned;
mod progress;
mod rules;
mod settings;
mod state;
mod tauri;
mod toast;
mod topbar;
mod widgets;

use crate::about::About;
use crate::audit::Audit;
use crate::confirm::Dialog;
use crate::inbox::Inbox;
use crate::planned::Planned;
use crate::progress::Progress;
use crate::rules::Rules;
use crate::settings::Settings;
use crate::state::{Clock, Editors, Tab, Ui};
use crate::toast::Toast;
use crate::topbar::TopBar;
use leptos::prelude::*;
use protocol::{
    ActionProgress, EVENT_ACTION_PROGRESS, EVENT_INBOX_UPDATED, EVENT_REFRESHING, EVENT_SHOW_ABOUT,
    InboxSnapshot,
};
use std::time::Duration;

/// Listens for the backend events, then loads the cached inbox once (the poller's first
/// refresh may have finished before), and ticks the clock once a second.
fn connect(ui: Ui, clock: Clock) {
    if !tauri::available() {
        ui.notice
            .set(Some("No backend: run PRism as a desktop app.".to_owned()));
        return;
    }
    set_interval(
        move || clock.0.set(format::now_ms()),
        Duration::from_secs(1),
    );
    demo::start(ui);
    leptos::task::spawn_local(async move {
        if let Err(error) = listen_all(ui).await {
            ui.notice.set(Some(error));
        }
        match api::get_inbox().await {
            // An event may have brought a newer one meanwhile.
            Ok(Some(snapshot)) if ui.snapshot.with_untracked(Option::is_none) => {
                ui.set_snapshot(snapshot);
            }
            // None: the poller's first refresh is still running; its event brings the inbox.
            Ok(_) => {}
            Err(error) => ui.notice.set(Some(error)),
        }
    });
}

async fn listen_all(ui: Ui) -> Result<(), String> {
    tauri::listen(EVENT_INBOX_UPDATED, move |snapshot: InboxSnapshot| {
        ui.set_snapshot(snapshot);
    })
    .await?;
    tauri::listen(EVENT_ACTION_PROGRESS, move |progress: ActionProgress| {
        ui.on_progress(progress);
    })
    .await?;
    tauri::listen(EVENT_REFRESHING, move |running: bool| {
        ui.refreshing.set(running);
    })
    .await?;
    tauri::listen(EVENT_SHOW_ABOUT, move |(): ()| ui.tab.set(Tab::About)).await
}

#[component]
fn App() -> impl IntoView {
    let ui = Ui::new();
    provide_context(ui);
    provide_context(Editors::new());
    let clock = Clock::new();
    provide_context(clock);
    connect(ui, clock);
    // Clicking the active tab keeps the page as it is.
    let tab = Memo::new(move |_| ui.tab.get());
    view! {
        <div class="app">
            <TopBar/>
            <main class="content">
                {move || match tab.get() {
                    Tab::Inbox => view! { <Inbox/> }.into_any(),
                    Tab::Planned => view! { <Planned/> }.into_any(),
                    Tab::Rules => view! { <Rules/> }.into_any(),
                    Tab::Settings => view! { <Settings/> }.into_any(),
                    Tab::Audit => view! { <Audit/> }.into_any(),
                    Tab::About => view! { <About/> }.into_any(),
                }}
            </main>
            // Bottom right, above the room of the floating action bar.
            <div class="dock">
                <Progress/>
                <Toast/>
            </div>
            <Dialog/>
        </div>
    }
}

fn main() {
    mount_to_body(App);
}
