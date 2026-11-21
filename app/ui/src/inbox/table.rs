//! The pull request table with keyboard navigation.

use crate::format::{age, ci_class, kind_class, kind_label, why_label};
use crate::progress::Mark;
use crate::state::{Clock, Ui};
use leptos::prelude::*;
use protocol::{PrItem, PrRef, PrState};
use wasm_bindgen::JsCast;

/// Whether a key press belongs to a form control (a row checkbox, say), not to the table.
fn in_control(ev: &web_sys::KeyboardEvent) -> bool {
    ev.target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|element| {
            matches!(
                element.tag_name().to_ascii_lowercase().as_str(),
                "input" | "textarea" | "select" | "button" | "a"
            )
        })
}

/// j/k or arrows move the open row, space/x selects it, o/Enter opens it on GitHub.
fn on_key(ui: Ui, ev: &web_sys::KeyboardEvent) {
    if in_control(ev) || ev.meta_key() || ev.ctrl_key() || ev.alt_key() {
        return;
    }
    let open = ui.open.get_untracked();
    let (next, previous, url) = ui.shown.with_untracked(|rows| {
        let position = open
            .as_ref()
            .and_then(|pr| rows.iter().position(|item| item.pr == *pr));
        let step = |delta: isize| {
            let next = position.map_or(0, |at| at.saturating_add_signed(delta));
            rows.get(next.min(rows.len().saturating_sub(1)))
                .map(|item| item.pr.clone())
        };
        let url = position
            .and_then(|at| rows.get(at))
            .map(|item| item.url.clone());
        (step(1), step(-1), url)
    });
    match ev.key().as_str() {
        "ArrowDown" | "j" => ui.open.set(next),
        "ArrowUp" | "k" => ui.open.set(previous),
        " " | "x" => {
            if let Some(pr) = &open {
                ui.toggle_selected(pr);
            }
        }
        "o" | "Enter" => {
            if let Some(url) = url {
                crate::api::open_url(&url);
            }
        }
        "Escape" => ui.open.set(None),
        _ => return,
    }
    ev.prevent_default();
}

#[component]
pub(crate) fn Table() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let all_selected = Memo::new(move |_| {
        ui.selected.with(|selected| {
            ui.shown.with(|rows| {
                !rows.is_empty() && rows.iter().all(|item| selected.contains(&item.pr))
            })
        })
    });
    let toggle_all = move |_| {
        let select = !all_selected.get_untracked();
        ui.shown.with_untracked(|rows| {
            ui.selected.update(|set| {
                for item in rows {
                    if select {
                        set.insert(item.pr.clone());
                    } else {
                        set.remove(&item.pr);
                    }
                }
            });
        });
    };
    let rows = move || {
        ui.shown
            .with(|rows| rows.iter().map(|item| item.pr.clone()).collect::<Vec<_>>())
    };
    view! {
        <div class="scroll" tabindex="0" on:keydown=move |ev| on_key(ui, &ev)>
            <table class="grid rows">
                <thead>
                    <tr>
                        <th>
                            <input
                                type="checkbox"
                                aria-label="Select all shown"
                                prop:checked=all_selected
                                on:change=toggle_all
                            />
                        </th>
                        <th>"Repo"</th>
                        <th>"#"</th>
                        <th>"Title"</th>
                        <th>"Author"</th>
                        <th>"Why"</th>
                        <th class="dotcol"></th>
                        <th class="dotcol">"CI"</th>
                        <th>"Age"</th>
                    </tr>
                </thead>
                <tbody>
                    <For each=rows key=PrRef::clone let:pr>
                        <Row pr/>
                    </For>
                </tbody>
            </table>
            <Show when=move || ui.snapshot.with(Option::is_some) && ui.shown.with(Vec::is_empty)>
                <p class="muted empty">"Nothing here."</p>
            </Show>
        </div>
    }
}

/// A row, keyed by its PR: the selection, mark and open state update in place, the cells only
/// re-render when the item changes.
#[component]
fn Row(pr: PrRef) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let clock = expect_context::<Clock>();
    let item = ui.item(pr.clone());
    let unread = Memo::new(move |_| {
        item.with(|item| {
            item.as_ref()
                .and_then(|item| item.notification.as_ref())
                .is_some_and(|n| n.unread)
        })
    });
    let (open_pr, click_pr, check_pr, change_pr) = (pr.clone(), pr.clone(), pr.clone(), pr.clone());
    view! {
        <tr
            class:open=move || ui.open.with(|open| open.as_ref() == Some(&open_pr))
            class:unread=unread
            on:click=move |_| ui.open.set(Some(click_pr.clone()))
        >
            <td on:click=|ev| ev.stop_propagation()>
                <input
                    type="checkbox"
                    aria-label=format!("Select {pr}")
                    prop:checked=move || ui.selected.with(|set| set.contains(&check_pr))
                    on:change=move |_| ui.toggle_selected(&change_pr)
                />
                <span class="markslot">
                    {move || ui.marks.with(|marks| marks.get(&pr).copied()).map(mark_view)}
                </span>
            </td>
            {move || item.with(|item| item.as_ref().map(|item| cells(clock, item, unread)))}
        </tr>
    }
}

fn cells(clock: Clock, item: &PrItem, unread: Memo<bool>) -> impl IntoView + use<> {
    let updated = item.updated_at.clone();
    let why = why_label(item);
    view! {
        <td class="cut" title=item.pr.repo.clone()>{item.pr.repo.clone()}</td>
        <td class="mono">{format!("#{}", item.pr.number)}</td>
        <td class="titlecell">{title(item)}</td>
        <td class="cut" title=item.author.clone()>{item.author.clone()}</td>
        <td class="cut muted" title=why.clone()>{why.clone()}</td>
        <td class="dotcol"><span class="dot unread-dot" class:on=unread></span></td>
        <td class="dotcol"><span class=format!("dot {}", ci_class(item.ci()))></span></td>
        <td class="muted nowrap">{move || age(&updated, clock.0.get())}</td>
    }
}

fn mark_view(mark: Mark) -> impl IntoView {
    match mark {
        Mark::Running => view! { <span class="spin on" title="running"></span> }.into_any(),
        Mark::Done => view! { <span class="rowmark good" title="done">"✓"</span> }.into_any(),
        Mark::Failed => view! { <span class="rowmark bad" title="failed">"✗"</span> }.into_any(),
    }
}

fn title(item: &PrItem) -> impl IntoView + use<> {
    // The text shrinks first (ellipsis); the badges keep their size.
    view! {
        <div class="titleline">
        <span class="ellipsis" title=item.title.clone()>{item.title.clone()}</span>
        {item
            .bump
            .as_ref()
            .map(|bump| {
                view! {
                    <span class=format!("badge {}", kind_class(bump.kind))>
                        {kind_label(bump.kind)}
                    </span>
                }
            })}
        {match item.state {
            PrState::Open => None,
            PrState::Merged => Some(view! { <span class="badge merged">"merged"</span> }),
            PrState::Closed => Some(view! { <span class="badge closed">"closed"</span> }),
        }}
        {item
            .duplicate_of
            .as_ref()
            .map(|pr| view! { <span class="badge dup">{format!("dup of #{}", pr.number)}</span> })}
        </div>
    }
}
