//! Left column: buckets and repositories with counts.

use crate::filter::{OwnerCount, RepoFilter, View, owner_counts, view_counts};
use crate::state::Ui;
use leptos::prelude::*;
use std::collections::BTreeSet;

#[component]
pub(crate) fn Sidebar() -> impl IntoView {
    let ui = expect_context::<Ui>();
    // Memos: the entries re-render only when a count changes, not on every keystroke.
    let views = Memo::new(move |_| {
        ui.filter.with(|f| {
            ui.snapshot
                .with(|s| s.as_ref().map_or_default(|s| view_counts(&s.items, f)))
        })
    });
    let owners = Memo::new(move |_| {
        ui.filter.with(|f| {
            ui.snapshot
                .with(|s| s.as_ref().map_or_default(|s| owner_counts(&s.items, f)))
        })
    });
    let expanded = RwSignal::new(BTreeSet::<String>::new());
    view! {
        <aside class="sidebar">
            <h4>"Buckets"</h4>
            {move || views.get().into_iter().map(|(view, count)| bucket_entry(ui, view, count)).collect_view()}
            <button
                class="entry"
                class:active=move || ui.filter.with(|f| f.involving_me_only)
                on:click=move |_| ui.filter.update(|f| f.involving_me_only = !f.involving_me_only)
                title="Hide pull requests that are only here because of the repository scope in settings.toml"
            >
                <span>"Only involving me"</span>
            </button>
            <h4>"Repositories"</h4>
            <button
                class="entry"
                class:active=move || ui.filter.with(|f| f.repo == RepoFilter::All)
                on:click=move |_| ui.filter.update(|filter| filter.repo = RepoFilter::All)
            >
                <span>"All repositories"</span>
            </button>
            {move || owners.get().into_iter().map(|owner| owner_entry(ui, expanded, owner)).collect_view()}
        </aside>
    }
}

fn bucket_entry(ui: Ui, view: View, count: usize) -> impl IntoView {
    view! {
        <button
            class="entry"
            class:active=move || ui.filter.with(|f| f.view == view)
            on:click=move |_| ui.set_view(view)
        >
            <span>{view.label()}</span>
            <span class="count">{count}</span>
        </button>
    }
}

fn click(ui: Ui, target: RepoFilter) {
    ui.filter
        .update(|filter| filter.repo = filter.repo.clicked(target));
}

/// An owner row (click filters to all its repositories, the caret expands it) and, when
/// expanded, its repository rows.
fn owner_entry(ui: Ui, expanded: RwSignal<BTreeSet<String>>, owner: OwnerCount) -> impl IntoView {
    let OwnerCount {
        owner,
        count,
        repos,
    } = owner;
    let target = RepoFilter::Owner(owner.clone());
    let (name, key, open_key) = (owner.clone(), owner.clone(), owner);
    let is_open = Memo::new(move |_| expanded.with(|set| set.contains(&open_key)));
    let rows = repos
        .into_iter()
        .map(|(repo, count)| repo_entry(ui, repo, count))
        .collect_view();
    let toggle = move |_| {
        expanded.update(|set| {
            if !set.remove(&key) {
                set.insert(key.clone());
            }
        });
    };
    view! {
        <div class="owner">
            <div class="owner-row">
                <button
                    class="caret"
                    aria-expanded=move || if is_open.get() { "true" } else { "false" }
                    aria-label=format!("Repositories of {name}")
                    on:click=toggle
                >
                    {move || if is_open.get() { "\u{25be}" } else { "\u{25b8}" }}
                </button>
                <button
                    class="entry"
                    class:active={
                        let target = target.clone();
                        move || ui.filter.with(|f| f.repo == target)
                    }
                    on:click=move |_| click(ui, target.clone())
                >
                    <span class="ellipsis grow">{name.clone()}</span>
                    <span class="count">{count}</span>
                </button>
            </div>
            <div class="repos" style:display=move || if is_open.get() { "block" } else { "none" }>
                {rows}
            </div>
        </div>
    }
}

fn repo_entry(ui: Ui, repo: String, count: usize) -> impl IntoView {
    let target = RepoFilter::Repo(repo.clone());
    let short = repo
        .split_once('/')
        .map_or(repo.as_str(), |(_, name)| name)
        .to_owned();
    view! {
        <button
            class="entry sub"
            class:active={
                let target = target.clone();
                move || ui.filter.with(|f| f.repo == target)
            }
            on:click=move |_| click(ui, target.clone())
            title=repo
        >
            <span class="ellipsis">{short}</span>
            <span class="count">{count}</span>
        </button>
    }
}
