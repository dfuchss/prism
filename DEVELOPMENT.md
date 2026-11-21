# Development

How to build and check PRism.

## Requirements

- [Rustup](https://rustup.rs/). The exact toolchain (Rust 1.100.0, edition 2024, clippy, rustfmt and the `wasm32-unknown-unknown` target) is pinned in `rust-toolchain.toml`; rustup installs it on first use. Release jobs add their own target triple with `rustup target add`.
- [Trunk](https://trunkrs.dev/) 0.22.1 and the [Tauri CLI](https://v2.tauri.app/reference/cli/) 2.12.4.
- For a real inbox: a GitHub token, see [Authentication](README.md#authentication).

```sh
rustup toolchain install                 # reads rust-toolchain.toml
cargo install trunk@0.22.1 tauri-cli@2.12.4 --locked
```

CI installs Trunk with the checksum-verifying `.github/scripts/install-trunk.sh` (the only place with the Trunk version). The local composite action `.github/actions/setup` (used by `check.yml` and `release.yml` after checkout) sets up the toolchain from `rust-toolchain.toml`, the Rust cache, Trunk and optionally the Tauri CLI (pinned prebuilt via `taiki-e/install-action`). Third-party actions are pinned to commit SHAs; Dependabot updates them weekly. `.github/scripts/version.sh` prints the version from `Cargo.toml`.

## Build and run

```sh
cd app/src-tauri
cargo tauri dev                          # run with live-reloading UI (trunk serve on port 1420)
cargo tauri build                        # release bundle: PRism.app (macOS 14+); AppImage on Linux (`--bundles appimage`); `--no-bundle` for the plain executable
```

The app embeds `app/ui/dist`, so the UI must be built (`cd app/ui && trunk build`) before `cargo build` of the app crate; `cargo tauri` does this for you.

## Architecture

A Cargo workspace; the UI crate is built by Trunk and is not a default member, so plain `cargo build` and `cargo test` skip it (`scripts/check.sh` runs its tests natively with `cargo test --workspace`).

| Crate | Path | Role |
|---|---|---|
| `protocol` | `crates/protocol` | Types shared by backend and UI, plus small pure helpers: `PrItem` (with `via_scope_only()`, `is_draft()`, `ci()`), `Action` (`Display`/`FromStr` for the rule-file strings; its JSON is stored in `audit.jsonl`), `Bump`/`UpdateKind`, `Capabilities` and `unavailable_reason`, bucket assignment, audit entries, `now_rfc3339`. Wire enums serialize kebab-case (except `Action`/`MergeMethod`, kept for `audit.jsonl`). Feature `test-support` adds `PrItem::sample` for other crates' tests. Compiled for native and wasm. |
| `prism-github` | `crates/github` | GitHub client (REST and GraphQL over `reqwest`). `client.rs` is the transport only (401 retry, rate limits, error messages); each domain module adds its own `impl Client`: `notifications`, `search`, `details`, `scope`, `inbox` (fetching, each with its parser), `mutations` (by node id, head-pinned), `rebase`, `workflows`, `actions` (`perform`). Also turns GitHub data into protocol types: `mapping` (enum strings), `bump` (bump titles and bodies), `duplicates` (`mark_duplicates`). Public API: `Client`, `Details`, `Error`, `mark_duplicates`. Tested against fixtures and `wiremock` (`src/http_tests.rs`). |
| `prism-rules` | `crates/rules` | The `rules.toml` schema (`Rule`, `RuleSet`; unknown keys are errors) and the pure rule engine: `evaluate` (plans carry the rule's position), `matches`, `auto_allowed` (safety gate), `validate` (also duplicate names), `load_rules`, the default `rules.toml`, repository globs (`glob_match`, `glob_problem`). |
| `prism` (app) | `app/src-tauri` | Tauri backend, see [Backend modules](#backend-modules). |
| `prism-ui` | `app/ui` | Leptos (CSR) front end with tabs Inbox, Planned, Rules, Settings, Audit and About. Talks to the backend through Tauri commands (`api.rs`, `tauri.rs`). |

Flow: the poller (`refresh.rs`) fetches the inbox, marks duplicates, stores the snapshot, emits `inbox-updated`, then runs the `auto = true` rules. Every action, manual or planned or automatic, goes through `exec::execute`: serialised by a lock, fresh details for head-bound actions, the gate, the call to the client, and an audit line.

### Backend modules

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
| `demo.rs` | Demo mode (`PRISM_DEMO=1`, see [Demo mode and screenshots](#demo-mode-and-screenshots)): the fictional inbox (`demo/data.rs`), simulated actions, the demo config directory and window. |

What happened to a pull request lives in three layers, from short to long-lived:

1. **Snapshot patch**: after a successful action, `InboxSnapshot::apply_action` updates the cached snapshot (a done thread vanishes, a merge shows as merged); its return value decides whether the UI is sent the snapshot (throttled during a batch: the first change at once, then at most every 250 ms, and once at the end if anything is still unsent). The next fetch replaces it.
2. **Overrides** (`overrides.rs`): what GitHub's eventually consistent endpoints may not show yet (threads hidden by mark done or unsubscribe, PRs merged or closed here) is applied to every fetch for 10 minutes, unless GitHub reports newer activity.
3. **Auto memo** (`exec::AutoMemo`): the actions `auto` rules executed, persisted in `auto-memo.json` (last 2000), so neither a poll nor a restart repeats them; failures and skip messages only for the session.

A fetch that started before a settings change is not stored (`AppState::settings_generation`); the refresh the change starts brings the new scope.

### Events

The event names are constants in `protocol` (`events.rs`); the backend sends them only from `app/src-tauri/src/events.rs`, the UI listens with those constants.

| Constant | Name | Payload | When |
|---|---|---|---|
| `EVENT_INBOX_UPDATED` | `inbox-updated` | `InboxSnapshot` | after every fetch, during a batch when a success changed the snapshot locally (at most every 250 ms, plus once at its end), after saving rules or settings |
| `EVENT_ACTION_PROGRESS` | `action-progress` | `ActionProgress` | batch start, each job start, each result |
| `EVENT_REFRESHING` | `refreshing` | `bool` | `true` when the first background refresh (after saving settings) starts, `false` when the last one ends |
| `EVENT_SHOW_ABOUT` | `show-about` | `()` | the macOS menu item "About PRism" |

## Check

```sh
scripts/check.sh                         # rustfmt, trunk build, clippy (native and wasm), tests
scripts/cross-lint.sh                    # clippy for Linux and Windows from a Mac (needs app/ui/dist)
```

`scripts/check.sh` must pass before every commit. The workspace lints (`Cargo.toml`, thresholds in `clippy.toml`) forbid `unsafe`, `unwrap`, `todo!` and printing; every crate opts in with `lints.workspace = true`. User-visible changes get an entry under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md).

`scripts/cross-lint.sh [linux|windows]` lints the `cfg(linux)` and `cfg(windows)` code on any host with stand-in build tools (C compiler, archiver, resource compiler, pkg-config) that only create empty outputs; nothing is linked or run. CI runs the real checks on macOS, Linux and Windows.

## Releasing

1. Set `version` in the workspace `Cargo.toml` (`tauri.conf.json` has no version of its own and inherits it) and let `Cargo.lock` follow (`cargo metadata`).
2. Move the `## [Unreleased]` entries of `CHANGELOG.md` under a new `## [X.Y.Z] - YYYY-MM-DD` heading, leave an empty `## [Unreleased]` above it and update the link references at the bottom (`.github/scripts/release-notes.sh` extracts the section up to the link references).
3. Run `scripts/check.sh`, commit and push to `main`.
4. Create a signed tag and push it: `git tag -s vX.Y.Z -m "vX.Y.Z"`, `git push origin vX.Y.Z`. The tag triggers the release workflow.
5. Check the published GitHub Release: artifacts, `SHA256SUMS` and the notes.

The `release` workflow (tags `v*`) checks that the tag equals the Cargo version, then builds unsigned artifacts and publishes a GitHub Release whose notes are the CHANGELOG section plus `.github/release-install.md`:

| Artifact | Built on |
|---|---|
| `prism-macos-arm64.zip`, `prism-macos-x86_64.zip` (zipped `PRism.app`, macOS 14+) | `macos-latest`, `--bundles app` |
| `prism-linux-x86_64.AppImage`, `prism-linux-aarch64.AppImage` and the plain binaries `prism-linux-x86_64`, `prism-linux-aarch64` | `ubuntu-24.04`, `ubuntu-24.04-arm` (the oldest glibc supported) |
| `prism-windows-x86_64.exe` (portable, no installer) | `windows-latest`, `--no-bundle` |
| `SHA256SUMS` | all of the above |

Notes: the Linux bundle config is `tauri.linux.conf.json` (AppImage only); the plain binary needs WebKitGTK 4.1 from the system. Never change the working directory at start-up (the AppImage runs from where the user started it). A release `[profile.release] strip = true` would break the wasm UI build (wasm-opt needs the target features), hence `strip = false` for `prism-ui`. The Windows build hides the console (`windows_subsystem`) and starts `gh` with `CREATE_NO_WINDOW`.

## Smoke test against GitHub

A read-only check against the real API with your token; it prints bucket counts, duplicates and update kinds and changes nothing:

```sh
cargo run -p prism-github --example smoke
PRISM_SCOPE_INCLUDE="owner/*,other/repo" cargo run -p prism-github --example smoke   # with scope
```

## Demo mode and screenshots

A development and debug option: `PRISM_DEMO=1` starts PRism with a fictional inbox (owners `acme`, `example-org`, `octo-labs`; viewer `demo-user`) instead of GitHub. No token, `gh` or network is used, and the real config directory is never read or written. Actions are simulated locally (about 300 ms each; merging `acme/api-gateway#88` always fails) and change the demo inbox like real ones; the audit log goes to the demo config directory.

| Variable | Meaning (only read with `PRISM_DEMO=1`) |
|---|---|
| `PRISM_CONFIG_DIR` | Config directory (default `prism-demo` in the temp directory). Missing `rules.toml` and `settings.toml` are seeded with the examples of `examples/rules` (all `auto = false`) and the scope `acme/*`. |
| `PRISM_DEMO_VIEW` | Screen to start on: `inbox` (detail pane of a failing PR), `bulk` (approves and merges the Ready bucket and stops after two jobs), `planned`, `rules`, `settings`, `audit`, `about`. |
| `PRISM_DEMO_THEME` | `light` (default) or `dark`. |
| `PRISM_DEMO_WINDOW_POSITION` | Window position `x,y`; the window is 1280×800 and stays on top. |

```sh
cd app/src-tauri && PRISM_DEMO=1 cargo tauri dev    # try the UI without a GitHub account
scripts/screenshots.sh                              # macOS: docs/screenshots/*.png
```

`scripts/screenshots.sh` builds a debug bundle with the bundle id `org.fuchss.prism.demo` in `target/screenshots` (`PRISM_SHOTS_TARGET_DIR`; `PRISM_SHOTS_SKIP_BUILD=1` reuses it), starts it once per screenshot in a temporary config directory, captures only its window (`screencapture -l`), scales the images to 1600 px width with ImageMagick (and `pngquant` when installed), and deletes the demo's data at the end. The terminal needs the Screen Recording permission (System Settings > Privacy & Security > Screen Recording).
