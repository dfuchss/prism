# Checks

```sh
scripts/check.sh                         # rustfmt, trunk build, clippy (native and wasm), tests
scripts/cross-lint.sh                    # clippy for Linux and Windows from a Mac (needs app/ui/dist)
```

`scripts/check.sh` must pass before every commit; the GitHub workflow `.github/workflows/check.yml` runs it too, on macOS, Linux and Windows. The workspace lints (`Cargo.toml`, thresholds in `clippy.toml`) forbid `unsafe`, `unwrap`, `todo!` and printing; every crate opts in with `lints.workspace = true`. User-visible changes get an entry under `## [Unreleased]` in [`CHANGELOG.md`](https://github.com/<<REPO>>/blob/main/CHANGELOG.md).

## Cross-lint

`scripts/cross-lint.sh [linux|windows]` lints the `cfg(linux)` and `cfg(windows)` code on any host with stand-in build tools (C compiler, archiver, resource compiler, pkg-config) that only create empty outputs; nothing is linked or run. CI runs the real checks on macOS, Linux and Windows.

## CI

- `.github/workflows/check.yml` runs `scripts/check.sh` on push to `main` and on pull requests.
- CI installs Trunk with the checksum-verifying `.github/scripts/install-trunk.sh` (the only place with the Trunk version). The local composite action `.github/actions/setup` (used by `check.yml` and `release.yml` after checkout) sets up the toolchain from `rust-toolchain.toml`, the Rust cache, Trunk and optionally the Tauri CLI (pinned prebuilt via `taiki-e/install-action`).
- Third-party actions are pinned to commit SHAs; Dependabot updates them weekly. `.github/scripts/version.sh` prints the version from `Cargo.toml`.
- `.github/workflows/docs.yml` mirrors `docs/` into the wiki, see [Releasing](Releasing#wiki).
