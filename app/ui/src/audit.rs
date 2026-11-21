//! The Audit tab: every executed action, newest first.

use crate::api;
use crate::format::action_label;
use crate::state::Ui;
use crate::widgets::Loader;
use leptos::prelude::*;
use protocol::AuditEntry;

const LIMIT: usize = 500;

#[component]
pub(crate) fn Audit() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let entries = RwSignal::new(Vec::<AuditEntry>::new());
    let loader = Loader::new();
    let load = move || loader.load(ui, api::audit_log(LIMIT), move |list| entries.set(list));
    load();
    view! {
        <section class="page">
            <div class="page-head">
                <h2>"Audit log"</h2>
                <span class="muted">{move || format!("{} entries", entries.with(Vec::len))}</span>
                <span class="spacer"></span>
                <button on:click=move |_| load()>"Reload"</button>
            </div>
            <div class="scroll">
                <table class="grid audit">
                    <thead>
                        <tr>
                            <th>"Time"</th>
                            <th></th>
                            <th>"PR"</th>
                            <th>"Action"</th>
                            <th>"By"</th>
                            <th>"Result"</th>
                        </tr>
                    </thead>
                    <tbody>{move || entries.get().into_iter().map(entry_row).collect_view()}</tbody>
                </table>
            </div>
        </section>
    }
}

fn entry_row(entry: AuditEntry) -> impl IntoView {
    let AuditEntry { at, rule, result } = entry;
    let pr = result.pr.to_string();
    let action = action_label(&result.action);
    let by = rule.unwrap_or_else(|| "manual".to_owned());
    view! {
        <tr>
            <td class="mono cut" title=at.clone()>{at.clone()}</td>
            <td><span class="dot" class:ci-green=result.ok class:ci-red=!result.ok></span></td>
            <td class="mono cut" title=pr.clone()>{pr.clone()}</td>
            <td class="cut" title=action.clone()>{action.clone()}</td>
            <td class="cut" title=by.clone()>{by.clone()}</td>
            <td class="cut" title=result.message.clone()>{result.message.clone()}</td>
        </tr>
    }
}
