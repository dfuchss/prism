# Development

How to build and check PRism quickly. The details are in the wiki: [Building](https://github.com/dfuchss/prism/wiki/Building) (macOS, Linux, Windows), [Architecture](https://github.com/dfuchss/prism/wiki/Architecture), [Checks](https://github.com/dfuchss/prism/wiki/Checks), [Debug options](https://github.com/dfuchss/prism/wiki/Debug-Options), [Screenshots](https://github.com/dfuchss/prism/wiki/Screenshots) and [Releasing](https://github.com/dfuchss/prism/wiki/Releasing).

## Requirements

- [Rustup](https://rustup.rs/); the toolchain (with clippy, rustfmt and the `wasm32-unknown-unknown` target) is pinned in `rust-toolchain.toml` and installed on first use.
- [Trunk](https://trunkrs.dev/) and the [Tauri CLI](https://v2.tauri.app/reference/cli/), in the versions given in [Building](https://github.com/dfuchss/prism/wiki/Building).
- On Linux the WebKitGTK and GTK development packages, on Windows the Microsoft C++ Build Tools and Git Bash: see [Building](https://github.com/dfuchss/prism/wiki/Building).

```sh
rustup toolchain install                 # reads rust-toolchain.toml
cargo install trunk@0.22.1 tauri-cli@2.12.4 --locked
```

## Build and run

```sh
cd app/src-tauri
cargo tauri dev                          # run with live-reloading UI
PRISM_DEMO=1 cargo tauri dev             # the same with a fictional inbox, no GitHub account needed
cargo tauri build                        # macOS: PRism.app, Linux: AppImage (--bundles appimage)
cargo tauri build --no-bundle            # Windows: target/release/prism.exe, Linux: target/release/prism
```

## Check

```sh
scripts/check.sh                         # everything CI checks: fmt, UI build, clippy, tests
```

`scripts/check.sh` must pass before every commit. User-visible changes get an entry under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md), in the format described in [Releasing](https://github.com/dfuchss/prism/wiki/Releasing#changelog).

The wiki pages live in [`docs/`](docs) and are mirrored into the wiki on every push to `main`; edit them there.
