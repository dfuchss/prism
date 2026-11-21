//! Reusable view pieces: result lines, the stale-safe loader, the TOML editor and the action
//! buttons.

use crate::actions::{Ask, OFFERS, Offer, availability, hint};
use crate::api;
use crate::format::result_text;
use crate::state::{Editor, Editors, Ui};
use leptos::prelude::*;
use protocol::{ActionResult, PrRef};
use std::future::Future;

/// One action result: a ✓ or ✗ and `action pr: message`.
pub(crate) fn result_line(result: &ActionResult) -> impl IntoView + use<> {
    view! {
        <div class="result" class:bad=!result.ok class:good=result.ok>
            <span class="mark">{if result.ok { "\u{2713}" } else { "\u{2717}" }}</span>
            <span>{result_text(result)}</span>
        </div>
    }
}

/// Runs loads of one component whose answers may arrive out of order: only the answer of the
/// latest load is applied, and none once the component is gone.
#[derive(Clone, Copy)]
pub(crate) struct Loader(StoredValue<u64>);

impl Loader {
    pub(crate) fn new() -> Self {
        Self(StoredValue::new(0))
    }

    /// Applies the outcome of `work` unless a newer load started (or the owner is gone).
    pub(crate) fn fetch<T: 'static>(
        self,
        work: impl Future<Output = T> + 'static,
        apply: impl FnOnce(T) + 'static,
    ) {
        let Some(generation) = self.0.try_update_value(|generation| {
            *generation += 1;
            *generation
        }) else {
            return;
        };
        leptos::task::spawn_local(async move {
            let outcome = work.await;
            if self.0.try_get_value() == Some(generation) {
                apply(outcome);
            }
        });
    }

    /// Like [`Loader::fetch`]; an error becomes the notice.
    pub(crate) fn load<T: 'static>(
        self,
        ui: Ui,
        work: impl Future<Output = Result<T, String>> + 'static,
        apply: impl FnOnce(T) + 'static,
    ) {
        self.fetch(work, move |outcome| match outcome {
            Ok(value) => apply(value),
            Err(error) => ui.notice.set(Some(error)),
        });
    }
}

/// A configuration file edited as TOML text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Doc {
    Rules,
    Settings,
}

impl Doc {
    fn title(self) -> &'static str {
        match self {
            Self::Rules => "Rules",
            Self::Settings => "Settings",
        }
    }

    fn editor(self, editors: Editors) -> Editor {
        match self {
            Self::Rules => editors.rules,
            Self::Settings => editors.settings,
        }
    }

    async fn load(self) -> Result<String, String> {
        match self {
            Self::Rules => api::get_rules().await,
            Self::Settings => api::get_settings().await,
        }
    }

    async fn save(self, toml: String) -> Result<Vec<String>, String> {
        match self {
            Self::Rules => api::save_rules(&toml).await,
            Self::Settings => api::save_settings(&toml).await,
        }
    }
}

/// A page editing `doc`: title, extra buttons, Save, the save outcome, the text and help
/// (`children`). The text lives in [`Ui`], so unsaved edits survive switching tabs; the file is
/// (re)loaded on mount only while there are none.
#[component]
pub(crate) fn TomlEditor(
    doc: Doc,
    /// A smaller text area (leaves room below).
    #[prop(optional)]
    short: bool,
    /// Buttons left of Save.
    #[prop(optional)]
    extra: Option<Children>,
    /// Runs once the text is loaded (or kept) and after each successful save.
    #[prop(optional, into)]
    on_ready: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let editor = doc.editor(expect_context::<Editors>());
    let alive = StoredValue::new(());
    let ready = move || {
        if let Some(on_ready) = on_ready
            && alive.try_get_value().is_some()
        {
            on_ready.run(());
        }
    };
    if editor.dirty.get_untracked() {
        ready();
    } else {
        Loader::new().load(ui, doc.load(), move |text| {
            if !editor.dirty.get_untracked() {
                editor.text.set(text.clone());
                editor.saved.set(Some(text));
            }
            ready();
        });
    }
    view! {
        <section class="page">
            <div class="page-head">
                <h2>{doc.title()}</h2>
                <span class="muted dirty" class:on=move || editor.dirty.get()>"Unsaved changes"</span>
                <span class="spacer"></span>
                {extra.map(|extra| extra())}
                <button
                    class="primary save"
                    disabled=move || editor.saving.get() || !editor.dirty.get()
                    on:click=move |_| save(doc, editor, ready)
                >
                    {move || if editor.saving.get() { "Saving..." } else { "Save" }}
                </button>
            </div>
            <Outcome outcome=editor.outcome/>
            <textarea
                class="editor mono"
                class:short=short
                spellcheck="false"
                prop:value=move || editor.text.get()
                on:input=move |ev| editor.text.set(event_target_value(&ev))
            ></textarea>
            {children()}
        </section>
    }
}

/// Saves the text; the state lives in `editor`, so it lands even when the tab was left.
fn save(doc: Doc, editor: Editor, saved: impl FnOnce() + 'static) {
    let text = editor.text.get_untracked();
    editor.saving.set(true);
    leptos::task::spawn_local(async move {
        let outcome = doc.save(text.clone()).await;
        let ok = outcome.is_ok();
        if ok {
            editor.saved.set(Some(text));
        }
        editor.outcome.set(Some(outcome));
        editor.saving.set(false);
        if ok {
            saved();
        }
    });
}

#[component]
fn Outcome(outcome: RwSignal<Option<Result<Vec<String>, String>>>) -> impl IntoView {
    move || {
        outcome.with(|outcome| {
            outcome.as_ref().map(|result| match result {
                Err(error) => view! { <pre class="result bad">{error.clone()}</pre> }.into_any(),
                Ok(warnings) => view! {
                    <div class="result good">"Saved."</div>
                    {warnings
                        .iter()
                        .map(|w| view! { <div class="result warn">{w.clone()}</div> })
                        .collect_view()}
                }
                .into_any(),
            })
        })
    }
}

/// Which of the manual actions to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    All,
    /// The ones of the selection bar.
    Primary,
    /// The ones of its "More" menu.
    Secondary,
}

/// The manual actions of `show` as buttons acting on `targets` (the open PR or the selected
/// ones).
#[component]
pub(crate) fn ActionButtons(
    #[prop(into)] targets: Signal<Vec<PrRef>>,
    #[prop(default = Group::All)] show: Group,
) -> impl IntoView {
    OFFERS
        .iter()
        .filter(move |offer| match show {
            Group::All => true,
            Group::Primary => offer.primary,
            Group::Secondary => !offer.primary,
        })
        .map(|offer| view! { <ActionButton offer targets/> })
        .collect_view()
}

/// A button running `offer` on `targets`: disabled while actions run, when there is no target
/// or when none supports the action; the tooltip names the reasons.
#[component]
fn ActionButton(offer: &'static Offer, targets: Signal<Vec<PrRef>>) -> impl IntoView {
    let ui = expect_context::<Ui>();
    let state = Memo::new(move |_| {
        targets.with(|prs| ui.with_items(prs, |items| availability(items, &offer.action)))
    });
    let blocked = move || {
        targets.with(Vec::is_empty)
            || state.with(|(supported, skipped)| *supported == 0 && !skipped.is_empty())
    };
    let click = move |_| {
        let prs = targets.get_untracked();
        let action = offer.action.clone();
        match offer.ask {
            Ask::Never => ui.run_or_confirm(prs, action),
            ask @ (Ask::Confirm | Ask::Comment(_)) => ui.confirm(prs, action, ask.comment_box()),
        }
    };
    view! {
        <button
            class:danger=offer.danger
            disabled=move || ui.locked.get() || blocked()
            title=move || state.with(|(supported, skipped)| hint(*supported, skipped))
            on:click=click
        >
            {offer.label}
        </button>
    }
}
