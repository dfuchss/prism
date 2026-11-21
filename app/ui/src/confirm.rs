//! The confirm dialog for merges, closes, comments and applying planned actions: a native modal
//! `<dialog>`, so the page behind is inert (no j/k in the table), Escape cancels and focus
//! returns to where it was.

use crate::actions::CommentBox;
use crate::state::{Pending, Task, Ui};
use leptos::html;
use leptos::prelude::*;
use protocol::Action;
use wasm_bindgen::JsValue;

#[component]
pub(crate) fn Dialog() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let dialog = NodeRef::<html::Dialog>::new();
    let open = Memo::new(move |_| ui.pending.with(Option::is_some));
    // Runs after the content rendered, so `showModal` focuses its `autofocus` element.
    Effect::new(move |_| {
        let Some(element) = dialog.get() else {
            return;
        };
        match (open.get(), element.open()) {
            (true, false) => {
                if let Err(error) = element.show_modal() {
                    web_sys::console::error_1(&error);
                }
            }
            (false, true) => element.close(),
            _ => {}
        }
    });
    view! {
        <dialog
            class="dialog"
            node_ref=dialog
            aria-labelledby="dialog-title"
            // Escape and `close()` end up here.
            on:close=move |_| ui.pending.set(None)
            // A click on the backdrop targets the dialog itself; the content is a child.
            on:click=move |ev| {
                if ev.target().map(JsValue::from) == ev.current_target().map(JsValue::from) {
                    ui.pending.set(None);
                }
            }
        >
            {move || ui.pending.get().map(|pending| view! { <Confirm pending/> })}
        </dialog>
    }
}

#[component]
fn Confirm(pending: Pending) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let comment = RwSignal::new(String::new());
    let comment_box = pending.comment;
    let task = pending.task;
    let missing =
        move || comment_box == CommentBox::Required && comment.with(|text| text.trim().is_empty());
    let accept = move |_| {
        ui.pending.set(None);
        start(ui, task.clone(), &comment.get_untracked());
    };
    let placeholder = match comment_box {
        CommentBox::Required => "Comment",
        CommentBox::Optional | CommentBox::None => "Optional comment",
    };
    view! {
        <div class="dialog-body">
            <h3 id="dialog-title">{pending.title}</h3>
            <ul class="dialog-list">
                {pending.lines.into_iter().map(|line| view! { <li>{line}</li> }).collect_view()}
            </ul>
            {(comment_box != CommentBox::None)
                .then(|| {
                    view! {
                        <textarea
                            placeholder=placeholder
                            autofocus=comment_box == CommentBox::Required
                            prop:value=move || comment.get()
                            on:input=move |ev| comment.set(event_target_value(&ev))
                        ></textarea>
                    }
                })}
            <div class="dialog-buttons">
                <button on:click=move |_| ui.pending.set(None)>"Cancel"</button>
                <button
                    class="primary"
                    autofocus=comment_box != CommentBox::Required
                    disabled=missing
                    on:click=accept
                >
                    "Confirm"
                </button>
            </div>
        </div>
    }
}

/// Runs the confirmed task with the text of the comment box: the closing comment, or the
/// comment to post.
fn start(ui: Ui, task: Task, comment: &str) {
    match task {
        Task::Act { prs, heads, action } => {
            let text = Some(comment.trim())
                .filter(|text| !text.is_empty())
                .map(str::to_owned);
            let action = match (action, text) {
                (Action::Close { comment: None }, text) => Action::Close { comment: text },
                (Action::Comment(_), Some(text)) => Action::Comment(text),
                (action, _) => action,
            };
            ui.run_seen(prs, heads, action);
        }
        Task::Planned(planned) => ui.run_planned(planned),
    }
}
