#!/usr/bin/env bash
# Prints the PRism version (the only place it is defined is the workspace Cargo.toml).
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "prism") | .version'
