//! The Rules tab: edit `rules.toml`.

use crate::widgets::{Doc, TomlEditor};
use leptos::prelude::*;

const HELP: &str = "actions: approve, merge[:squash|merge|rebase], approve-and-merge, \
auto-merge, close[:\"text\"], comment:\"text\", mark-read, mark-done, unsubscribe, rerun-failed, \
rebase (Dependabot: comment, Renovate: tick its rebase checkbox, others: update the branch)";

const FIELDS: &str = "fields: name, authors, repos (globs), title (regex), update \
(patch|minor|major|unknown), checks (green|any), \
state (clean|blocked|unstable|dirty|behind|draft|has-hooks|unknown), duplicate (true|false), involved (true = only PRs that involve me, \
false = only PRs added by the scope in settings), actions, auto, allow_unknown (true = auto rules may also merge unknown updates)";

#[component]
pub(crate) fn Rules() -> impl IntoView {
    view! {
        <TomlEditor doc=Doc::Rules>
            <p class="muted help">{HELP}</p>
            <p class="muted help">{FIELDS}</p>
            <p class="muted help">
                "Rules with auto = true run on every refresh, but never merge major updates (or unknown ones without allow_unknown = true) or pull requests without green checks."
            </p>
        </TomlEditor>
    }
}
