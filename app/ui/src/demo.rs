//! Demo mode (`PRISM_DEMO=1`, see the backend's `demo.rs`): once the first inbox is shown, opens
//! the screen named by `PRISM_DEMO_VIEW`, for reproducible screenshots. Does nothing otherwise.

use crate::api;
use crate::filter::View;
use crate::state::{Tab, Ui};
use leptos::prelude::*;
use protocol::{Action, Bucket, CheckState};

/// Asks the backend for the demo view and applies it once the inbox has arrived.
pub(crate) fn start(ui: Ui) {
    let view = RwSignal::new(None::<String>);
    leptos::task::spawn_local(async move {
        if let Ok(Some(name)) = api::demo_view().await {
            view.set(Some(name));
        }
    });
    Effect::new(move |done: Option<bool>| {
        if done == Some(true) {
            return true;
        }
        let ready = ui.snapshot.with(Option::is_some);
        match view.get() {
            Some(name) if ready => {
                untrack(|| show(ui, &name));
                true
            }
            _ => false,
        }
    });
}

fn show(ui: Ui, name: &str) {
    match name {
        "inbox" => open_failing(ui),
        "bulk" => merge_ready(ui),
        "planned" => ui.tab.set(Tab::Planned),
        "rules" => ui.tab.set(Tab::Rules),
        "settings" => ui.tab.set(Tab::Settings),
        "audit" => ui.tab.set(Tab::Audit),
        "about" => ui.tab.set(Tab::About),
        _ => {}
    }
}

/// Opens the newest pull request with failing checks in the detail pane.
fn open_failing(ui: Ui) {
    let failing = ui.shown.with(|rows| {
        rows.iter()
            .find(|item| item.checks.iter().any(|c| c.state == CheckState::Failure))
            .map(|item| item.pr.clone())
    });
    ui.open.set(failing);
}

/// Shows the Ready bucket, selects all of it and approves and merges it.
fn merge_ready(ui: Ui) {
    ui.set_view(View::Bucket(Bucket::Ready));
    let (prs, heads): (Vec<_>, Vec<_>) = ui.shown.with(|rows| {
        rows.iter()
            .map(|item| (item.pr.clone(), item.head_oid.clone()))
            .unzip()
    });
    ui.selected.set(prs.iter().cloned().collect());
    ui.run_seen(prs, heads, Action::ApproveAndMerge(None));
}
