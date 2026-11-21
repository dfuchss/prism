# Architecture

A Cargo workspace; the UI crate is built by Trunk and is not a default member, so plain `cargo build` and `cargo test` skip it (`scripts/check.sh` runs its tests natively with `cargo test --workspace`).

## Crates

| Crate | Path | Role |
|---|---|---|
| `protocol` | `crates/protocol` | Types shared by backend and UI, plus small pure helpers: `PrItem` (with `via_scope_only()`, `is_draft()`, `ci()`), `Action` (`Display`/`FromStr` for the rule-file strings; its JSON is stored in `audit.jsonl`), `Bump`/`UpdateKind`, `Capabilities` and `unavailable_reason`, bucket assignment, audit entries, `now_rfc3339`. Wire enums serialize kebab-case (except `Action`/`MergeMethod`, kept for `audit.jsonl`). Feature `test-support` adds `PrItem::sample` for other crates' tests. Compiled for native and wasm. |
| `prism-github` | `crates/github` | GitHub client (REST and GraphQL over `reqwest`). `client.rs` is the transport only (401 retry, rate limits, error messages); each domain module adds its own `impl Client`: `notifications`, `search`, `details`, `scope`, `inbox` (fetching, each with its parser), `mutations` (by node id, head-pinned), `rebase`, `workflows`, `actions` (`perform`). Also turns GitHub data into protocol types: `mapping` (enum strings), `bump` (bump titles and bodies), `duplicates` (`mark_duplicates`). Public API: `Client`, `Details`, `Error`, `mark_duplicates`. Tested against fixtures and `wiremock` (`src/http_tests.rs`). |
| `prism-rules` | `crates/rules` | The `rules.toml` schema (`Rule`, `RuleSet`; unknown keys are errors) and the pure rule engine: `evaluate` (plans carry the rule's position), `matches`, `auto_allowed` (safety gate), `validate` (also duplicate names), `load_rules`, the default `rules.toml`, repository globs (`glob_match`, `glob_problem`). |
| `prism` (app) | `app/src-tauri` | Tauri backend, see [Backend modules](#backend-modules). |
| `prism-ui` | `app/ui` | Leptos (CSR) front end with tabs Inbox, Planned, Rules, Settings, Audit and About. Talks to the backend through Tauri commands (`api.rs`, `tauri.rs`). |

Flow: the poller (`refresh.rs`) fetches the inbox, marks duplicates, stores the snapshot, emits `inbox-updated`, then runs the `auto = true` rules. Every action, manual or planned or automatic, goes through `exec::execute`: serialised by a lock, fresh details for head-bound actions, the gate, the call to the client, and an audit line.

## Backend modules

| Module | Role |
|---|---|
| `state.rs` | `AppState`, shared by all commands and tasks, and its locking rules (doc comment "Locking": `refresh_lock` → auto pass gate → `action_lock`; `std` guards never across `.await`, never nested). |
| `commands.rs` | The Tauri commands; only fallible ones return `Result`, file I/O runs on the blocking pool (`config::blocking`). |
| `events.rs` | Every event sent to the UI (see [Events](#events)). |
| `refresh.rs` | Fetching and storing the inbox, the auto pass (`PassGate`), the poller. |
| `exec.rs` | The action runner with its safety checks, the audit lines and the `AutoMemo`. |
| `overrides.rs` | Local state changes GitHub does not report yet. |
| `scope.rs` | The repository scope: cached repository list (`RepoCache`), rescoping, scope PRs. |
| `config.rs` | The files in the config directory: `rules.toml`, `settings.toml`, `audit.jsonl` (rotated to `audit.jsonl.1` at 1 MB), `auto-memo.json`. |
| `settings.rs` | The `settings.toml` schema. |
| `menu.rs` | The macOS app menu (About). |
| `source.rs` | Where the inbox comes from: the GitHub `Client`, or the demo data. The only place that tells them apart. |
| `demo.rs` | Demo mode (`PRISM_DEMO=1`, see [Debug options](Debug-Options)): the fictional inbox (`demo/data.rs`), simulated actions, the demo config directory and window. |

## What PRism remembers

What happened to a pull request lives in three layers, from short to long-lived:

1. **Snapshot patch**: after a successful action, `InboxSnapshot::apply_action` updates the cached snapshot (a done thread vanishes, a merge shows as merged); its return value decides whether the UI is sent the snapshot (throttled during a batch: the first change at once, then at most every 250 ms, and once at the end if anything is still unsent). The next fetch replaces it.
2. **Overrides** (`overrides.rs`): what GitHub's eventually consistent endpoints may not show yet (threads hidden by mark done or unsubscribe, PRs merged or closed here) is applied to every fetch for 10 minutes, unless GitHub reports newer activity.
3. **Auto memo** (`exec::AutoMemo`): the actions `auto` rules executed, persisted in `auto-memo.json` (last 2000), so neither a poll nor a restart repeats them; failures and skip messages only for the session.

A fetch that started before a settings change is not stored (`AppState::settings_generation`); the refresh the change starts brings the new scope.

## Events

The event names are constants in `protocol` (`events.rs`); the backend sends them only from `app/src-tauri/src/events.rs`, the UI listens with those constants.

| Constant | Name | Payload | When |
|---|---|---|---|
| `EVENT_INBOX_UPDATED` | `inbox-updated` | `InboxSnapshot` | after every fetch, during a batch when a success changed the snapshot locally (at most every 250 ms, plus once at its end), after saving rules or settings |
| `EVENT_ACTION_PROGRESS` | `action-progress` | `ActionProgress` | batch start, each job start, each result |
| `EVENT_REFRESHING` | `refreshing` | `bool` | `true` when the first background refresh (after saving settings) starts, `false` when the last one ends |
| `EVENT_SHOW_ABOUT` | `show-about` | `()` | the macOS menu item "About PRism" |
