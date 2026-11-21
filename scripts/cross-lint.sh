#!/usr/bin/env bash
# Clippy-lints the Linux and Windows code paths from a Mac (or any host) without those systems'
# SDKs: the C compiler, archiver, resource compiler and pkg-config are replaced by stand-ins that
# only create empty outputs. Only `cargo clippy` (no linking) works this way; it never runs
# anything. Needs rustup and app/ui/dist (run `cd app/ui && trunk build` first).
# Usage: scripts/cross-lint.sh [linux|windows]...   (default: both)
set -euo pipefail
cd "$(dirname "$0")/.."

[ -d app/ui/dist ] || { echo "app/ui/dist is missing: run 'cd app/ui && trunk build'" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
stubs="${work}/stubs"
mkdir -p "${stubs}"

# cc: create (truncate) the file after -o.
cat > "${stubs}/cc" <<'STUB'
#!/usr/bin/env bash
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then : > "$2"; fi
  shift
done
exit 0
STUB
# ar (Linux): every *.a argument becomes an empty archive.
cat > "${stubs}/ar-linux" <<'STUB'
#!/usr/bin/env bash
for arg in "$@"; do
  case "${arg}" in *.a) printf '!<arch>\n' > "${arg}" ;; esac
done
exit 0
STUB
# lib.exe style (Windows): the archive is the argument of -out:.
cat > "${stubs}/ar-windows" <<'STUB'
#!/usr/bin/env bash
for arg in "$@"; do
  case "${arg}" in -out:*|/out:*) printf '!<arch>\n' > "${arg#*:}" ;; esac
done
exit 0
STUB
cat > "${stubs}/llvm-rc" <<'STUB'
#!/usr/bin/env bash
# embed-resource probes the compiler with `-V /?`.
for arg in "$@"; do
  if [ "${arg}" = "/?" ]; then
    echo "OVERVIEW: Resource Converter"
    echo "/no-preprocess"
    exit 0
  fi
done
while [ "$#" -gt 0 ]; do
  case "$1" in /fo) : > "$2" ;; /fo*) : > "${1#/fo}" ;; esac
  shift
done
exit 0
STUB
cat > "${stubs}/pkg-config" <<'STUB'
#!/usr/bin/env bash
for arg in "$@"; do
  [ "${arg}" = "--modversion" ] && echo 99.0
done
exit 0
STUB
chmod +x "${stubs}"/*

lint() {
  local target="$1" ar_stub="$2" underscored="${1//-/_}"
  rustup target add "${target}" > /dev/null
  echo "==> clippy ${target}"
  env "CC_${underscored}=${stubs}/cc" "AR_${underscored}=${stubs}/${ar_stub}" \
    "RC_${underscored}=${stubs}/llvm-rc" PKG_CONFIG="${stubs}/pkg-config" PKG_CONFIG_ALLOW_CROSS=1 \
    CARGO_TARGET_DIR="${CROSS_LINT_TARGET_DIR:-${PWD}/target/cross-lint-${target}}" \
    cargo clippy --target "${target}" --workspace --exclude prism-ui --all-targets -- -D warnings
}

targets=("$@")
[ "${#targets[@]}" -gt 0 ] || targets=(linux windows)
for platform in "${targets[@]}"; do
  case "${platform}" in
    linux) lint x86_64-unknown-linux-gnu ar-linux ;;
    windows) lint x86_64-pc-windows-msvc ar-windows ;;
    *) echo "usage: scripts/cross-lint.sh [linux|windows]..." >&2; exit 2 ;;
  esac
done
