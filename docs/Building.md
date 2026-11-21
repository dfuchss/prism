# Building from source

Every platform needs [Rustup](https://rustup.rs/), [Trunk](https://trunkrs.dev/) and the [Tauri CLI](https://v2.tauri.app/reference/cli/). The exact toolchain (Rust 1.100.0, edition 2024, clippy, rustfmt and the `wasm32-unknown-unknown` target) is pinned in `rust-toolchain.toml`; rustup installs it on first use. Release jobs add their own target triple with `rustup target add`.

```sh
rustup toolchain install                 # reads rust-toolchain.toml
cargo install trunk@0.22.1 tauri-cli@2.12.4 --locked

git clone https://github.com/<<REPO>>.git
cd prism/app/src-tauri
```

For a real inbox you need a GitHub token, see [Settings](Settings#authentication); without one, try the UI in [demo mode](Debug-Options): `PRISM_DEMO=1 cargo tauri dev`.

`cargo tauri dev` runs PRism with a live-reloading UI everywhere (`trunk serve` on port 1420). The app embeds `app/ui/dist`, so the UI must be built (`cd app/ui && trunk build`) before `cargo build` of the app crate; `cargo tauri` does this for you. The checks every commit must pass are in [Checks](Checks).

## macOS

Release builds need macOS 14 or later.

```sh
cargo tauri build --bundles app          # → target/release/bundle/macos/PRism.app
```

Copy `PRism.app` to `/Applications`. The app is not signed; the first time, open it as described in [macOS](macOS#install).

## Linux

Install the WebKitGTK and GTK development packages (Debian/Ubuntu: `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`), then:

```sh
cargo tauri build --bundles appimage     # → target/release/bundle/appimage/prism_<version>_<arch>.AppImage
cargo tauri build --no-bundle            # → target/release/prism (uses the system's WebKitGTK 4.1 and GTK 3)
```

The Linux bundle config is `tauri.linux.conf.json` (AppImage only). `scripts/check-appimage.sh <file>` checks a built AppImage.

## Windows

Install the Microsoft C++ Build Tools and [Git Bash](https://git-scm.com/) (the scripts expect it); the WebView2 Runtime is included in Windows 11 and current Windows 10.

```sh
cargo tauri build --no-bundle            # → target/release/prism.exe
```

The Windows build hides the console (`windows_subsystem`) and starts `gh` with `CREATE_NO_WINDOW`.

## Notes

- Never change the working directory at start-up (the AppImage runs from where the user started it).
- A release `[profile.release] strip = true` would break the wasm UI build (wasm-opt needs the target features), hence `strip = false` for `prism-ui`.
- A read-only smoke test against the real GitHub API with your token prints bucket counts, duplicates and update kinds and changes nothing:

```sh
cargo run -p prism-github --example smoke
PRISM_SCOPE_INCLUDE="owner/*,other/repo" cargo run -p prism-github --example smoke   # with scope
```
