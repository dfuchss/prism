//! The floating action bar over the table while rows are selected.

use crate::state::Ui;
use crate::widgets::{ActionButtons, Group};
use leptos::prelude::*;
use std::collections::BTreeSet;

/// Overlaid at the bottom of the table (it takes no room, so nothing moves when it appears):
/// the selection, the main actions, a menu with the rest and clear.
#[component]
pub(crate) fn BulkBar() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let selected = Memo::new(move |_| ui.selected.with(BTreeSet::len));
    let any = move || selected.get() > 0;
    let hidden = move || selected.get().saturating_sub(ui.chosen.with(Vec::len));
    let label = move || match hidden() {
        0 => format!("{} selected", selected.get()),
        hidden => format!("{} selected ({hidden} hidden)", selected.get()),
    };
    let tip = move || {
        (hidden() > 0)
            .then(|| "Selected pull requests the filter hides are not acted on".to_owned())
    };
    view! {
        <Show when=any>
            <div class="bulk" role="toolbar" aria-label="Selected pull requests">
                <strong class="selected-count" title=tip>{label}</strong>
                <ActionButtons targets=ui.chosen show=Group::Primary/>
                <MoreMenu/>
                <button
                    class="ghost clear"
                    title="Clear selection"
                    aria-label="Clear selection"
                    on:click=move |_| ui.selected.set(BTreeSet::new())
                >
                    "\u{d7}"
                </button>
            </div>
        </Show>
    }
}

/// The "More" button and its popover above it; closes on Escape, a click elsewhere or a choice.
#[component]
fn MoreMenu() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let open = RwSignal::new(false);
    let handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" && open.get_untracked() {
            open.set(false);
        }
    });
    on_cleanup(move || handle.remove());
    view! {
        <span class="more">
            <button
                aria-haspopup="true"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|open| *open = !*open)
            >
                "More \u{25be}"
            </button>
            <Show when=move || open.get()>
                // Transparent, below the menu: a click anywhere else only closes it.
                <div class="more-backdrop" on:click=move |_| open.set(false)></div>
                <div class="more-menu" role="group" on:click=move |_| open.set(false)>
                    <ActionButtons targets=ui.chosen show=Group::Secondary/>
                </div>
            </Show>
        </span>
    }
}
