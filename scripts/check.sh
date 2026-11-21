#!/usr/bin/env bash
# Runs every check the code must pass (also in CI): formatting, the UI build, clippy with the
# workspace lints (native and wasm, warnings are errors) and the tests.
#
# Needs Rust with the wasm32-unknown-unknown target and Trunk.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "rustfmt"
cargo fmt --all --check

# The app embeds app/ui/dist, so the UI is built before the app is compiled.
step "trunk build"
(cd app/ui && trunk build)

step "clippy (native)"
cargo clippy --workspace --all-targets -- -D warnings

step "clippy (wasm UI)"
cargo clippy -p prism-ui --target wasm32-unknown-unknown -- -D warnings

step "tests"
# The UI tests (pure logic such as filter.rs) run natively too.
cargo test --workspace

step "all checks passed"
