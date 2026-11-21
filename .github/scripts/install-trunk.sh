#!/usr/bin/env bash
# Installs Trunk into Cargo's bin folder (INSTALL_DIR overrides it) after checking the SHA-256 of the release
# archive. Works on Linux (x86_64, aarch64; static musl builds), macOS (arm64, x86_64) and Windows x86_64
# (Git Bash). The version and the checksums live only here.
set -euo pipefail

version=v0.22.1

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)
    target=x86_64-unknown-linux-musl
    sha256=a1fe5cd6b053f5a76f66b598d296022a7f06f56def6013cc16cd6897d53f5363 ;;
  Linux-aarch64)
    target=aarch64-unknown-linux-musl
    sha256=8c84a939aaf48bbdac2d78195c932adb319e610fb2cbf791bae07b2660cf3e46 ;;
  Darwin-arm64)
    target=aarch64-apple-darwin
    sha256=18e381936c848700fefb9ee3f64f362419fab700200eefee323ebc0d00a8e9db ;;
  Darwin-x86_64)
    target=x86_64-apple-darwin
    sha256=55b5cf2d6a23201f555cc561cc07157989581cc720a260505f8e98aae67abcf7 ;;
  MINGW*-x86_64 | MSYS*-x86_64)
    target=x86_64-pc-windows-msvc
    sha256=1cc65012a9df60b02db948f5a3f0601e594ce952efb1cfd23147c9021c9f7d75 ;;
  *) echo "no Trunk build for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

case "${target}" in
  *windows*) extension=zip ;;
  *) extension=tar.gz ;;
esac

archive_name="trunk-${target}.${extension}"
download_directory="$(mktemp -d)"
trap 'rm -rf "${download_directory}"' EXIT
archive="${download_directory}/${archive_name}"
bin_directory="${INSTALL_DIR:-${CARGO_HOME:-${HOME}/.cargo}/bin}"

curl -fsSL -o "${archive}" "https://github.com/trunk-rs/trunk/releases/download/${version}/${archive_name}"
# Compare digests as strings: the --check flags differ between GNU sha256sum, the BSD sha256sum
# of newer macOS and shasum.
if command -v shasum > /dev/null; then
  actual="$(shasum -a 256 "${archive}" | awk '{print $1}')"
else
  actual="$(sha256sum "${archive}" | awk '{print $1}')"
fi
if [ "${actual}" != "${sha256}" ]; then
  echo "checksum mismatch for ${archive_name}: expected ${sha256}, got ${actual}" >&2
  exit 1
fi

mkdir -p "${bin_directory}"
if [ "${extension}" = zip ]; then
  unzip -o -q "${archive}" trunk.exe -d "${bin_directory}"
else
  tar -xz -f "${archive}" -C "${bin_directory}" trunk
fi
"${bin_directory}/trunk" --version
