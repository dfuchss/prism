//! Results of actions and backend errors, shown at the bottom right.

use crate::state::Ui;
use crate::widgets::result_line;
use leptos::prelude::*;

#[component]
pub(crate) fn Toast() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let visible = move || ui.notice.with(Option::is_some) || !ui.results.with(Vec::is_empty);
    let dismiss = move |_| {
        ui.notice.set(None);
        ui.results.set(Vec::new());
    };
    view! {
        <Show when=visible>
            <aside class="toast">
                <div class="toast-head">
                    <strong>"Results"</strong>
                    <button class="ghost" on:click=dismiss>"Dismiss"</button>
                </div>
                {move || ui.notice.get().map(|text| view! { <div class="result bad">{text}</div> })}
                <div class="toast-list">
                    {move || ui.results.with(|results| results.iter().map(result_line).collect_view())}
                </div>
            </aside>
        </Show>
    }
}
