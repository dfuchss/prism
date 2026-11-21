//! The Planned tab: what the rules would do, to review and apply.

use crate::api;
use crate::format::action_label;
use crate::state::Ui;
use crate::widgets::Loader;
use leptos::prelude::*;
use protocol::PlannedAction;
use std::collections::BTreeSet;

/// Identifies a planned action across reloads (list positions shift when the inbox changes).
fn key(planned: &PlannedAction) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        planned.rule_index, planned.rule, planned.pr, planned.action, planned.head_oid
    )
}

#[component]
pub(crate) fn Planned() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let planned = RwSignal::new(Vec::<PlannedAction>::new());
    let unchecked = RwSignal::new(BTreeSet::<String>::new());
    // Reload on mount and whenever the inbox changes. New entries start checked; unchecked
    // ones stay unchecked across reloads.
    let loader = Loader::new();
    Effect::new(move |_| {
        ui.snapshot.track();
        loader.load(ui, api::planned_actions(), move |list| {
            let keys: BTreeSet<String> = list.iter().map(key).collect();
            unchecked.update(|set| set.retain(|k| keys.contains(k)));
            planned.set(list);
        });
    });
    let apply = move |_| {
        let chosen: Vec<PlannedAction> = unchecked.with_untracked(|skip| {
            planned.with_untracked(|planned| {
                planned
                    .iter()
                    .filter(|action| !skip.contains(&key(action)))
                    .cloned()
                    .collect()
            })
        });
        if !chosen.is_empty() {
            ui.confirm_planned(chosen);
        }
    };
    view! {
        <section class="page">
            <div class="page-head">
                <h2>"Planned actions"</h2>
                <span class="muted">
                    {move || format!("{} suggested by your rules", planned.with(Vec::len))}
                </span>
                <span class="spacer"></span>
                <button
                    class="primary"
                    disabled=move || {
                        ui.locked.get() || unchecked.with(BTreeSet::len) >= planned.with(Vec::len)
                    }
                    on:click=apply
                >
                    "Apply selected"
                </button>
            </div>
            <PlannedTable planned unchecked/>
        </section>
    }
}

#[component]
fn PlannedTable(
    planned: RwSignal<Vec<PlannedAction>>,
    unchecked: RwSignal<BTreeSet<String>>,
) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let rows = move || {
        planned
            .get()
            .into_iter()
            .map(|item| {
                let title = ui.with_item(&item.pr, |pr| pr.map_or_default(|pr| pr.title.clone()));
                let pr = item.pr.to_string();
                let id = key(&item);
                let checked_id = id.clone();
                let toggle = move |_| {
                    unchecked.update(|set| {
                        if !set.remove(&id) {
                            set.insert(id.clone());
                        }
                    });
                };
                view! {
                    <tr>
                        <td>
                            <input
                                type="checkbox"
                                prop:checked=move || !unchecked.with(|set| set.contains(&checked_id))
                                on:change=toggle
                            />
                        </td>
                        <td class="cut" title=item.rule.clone()>{item.rule.clone()}</td>
                        <td class="mono cut" title=pr.clone()>{pr.clone()}</td>
                        <td class="cut" title=title.clone()>{title.clone()}</td>
                        <td class="nowrap"><span class="badge">{action_label(&item.action)}</span></td>
                    </tr>
                }
            })
            .collect_view()
    };
    view! {
        <div class="scroll">
            <table class="grid planned">
                <thead>
                    <tr>
                        <th></th>
                        <th>"Rule"</th>
                        <th>"PR"</th>
                        <th>"Title"</th>
                        <th>"Action"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}
