//! The Inbox tab: sidebar, table with its floating action bar, and detail pane.

mod bulk;
mod detail;
mod sidebar;
mod table;

use crate::state::Ui;
use bulk::BulkBar;
use detail::Detail;
use leptos::prelude::*;
use sidebar::Sidebar;
use table::Table;

#[component]
pub(crate) fn Inbox() -> impl IntoView {
    let ui = expect_context::<Ui>();
    // Setting the same PR again (j at the last row) keeps the pane as it is.
    let open = Memo::new(move |_| ui.open.get());
    view! {
        <div class="inbox" class:with-detail=move || open.with(Option::is_some)>
            <Sidebar/>
            <div class="center">
                <Toolbar/>
                <Table/>
                <BulkBar/>
            </div>
            {move || open.get().map(|pr| view! { <Detail pr/> })}
        </div>
    }
}

/// Text search and the unread/bots/humans toggles.
#[component]
fn Toolbar() -> impl IntoView {
    use crate::filter::Who;
    let ui = expect_context::<Ui>();
    let toggle = move |label: &'static str, active: Memo<bool>, flip: fn(Ui)| {
        view! {
            <button class="chip" class:active=move || active.get() on:click=move |_| flip(ui)>
                {label}
            </button>
        }
    };
    let unread = Memo::new(move |_| ui.filter.with(|f| f.unread_only));
    let bots = Memo::new(move |_| ui.filter.with(|f| f.who == Who::Bots));
    let humans = Memo::new(move |_| ui.filter.with(|f| f.who == Who::Humans));
    view! {
        <div class="toolbar">
            <input
                class="search"
                type="search"
                placeholder="Search title, repo, author, #number"
                prop:value=move || ui.filter.with(|f| f.search.clone())
                on:input=move |ev| {
                    let text = event_target_value(&ev);
                    ui.filter.update(|filter| filter.search = text);
                }
            />
            {toggle("Unread", unread, |ui| ui.filter.update(|f| f.unread_only = !f.unread_only))}
            {toggle("Bots", bots, |ui| ui.toggle_who(Who::Bots))}
            {toggle("Humans", humans, |ui| ui.toggle_who(Who::Humans))}
        </div>
    }
}
