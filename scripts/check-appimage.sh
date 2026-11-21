#!/usr/bin/env bash
# Checks a built AppImage: its desktop entry, icon and binary and the AppImage type 2 magic. Fails
# unless it is given exactly one file, so a glob that matches no or several AppImages fails too.
# Extracting works without FUSE. Used by the release workflow.
# Usage: scripts/check-appimage.sh <AppImage>
set -euo pipefail

fail() { echo "$*" >&2; exit 1; }

[ "$#" -eq 1 ] && [ -f "$1" ] || fail "expected one AppImage, found: $*"
appimage="$(realpath "$1")"

extract_directory="$(mktemp -d)"
trap 'rm -rf "${extract_directory}"' EXIT
(cd "${extract_directory}" && APPIMAGE_EXTRACT_AND_RUN=1 "${appimage}" --appimage-extract > /dev/null)
contents="${extract_directory}/squashfs-root"

desktop_entry="$(find "${contents}" -maxdepth 1 -name "*.desktop" | head -n 1)"
[ -f "${desktop_entry}" ] || fail "the desktop entry is missing"
grep -q '^Exec=' "${desktop_entry}" || fail "the desktop entry has no Exec line"
[ -x "${contents}/usr/bin/prism" ] || fail "usr/bin/prism is missing"
ls "${contents}"/usr/share/icons/hicolor/*/apps/*.png > /dev/null 2>&1 || fail "the icon is missing"

# The AppImage type 2 magic at byte 8: AI followed by 0x02.
[ "$(xxd -s 8 -l 3 -p "${appimage}")" = "414902" ] || fail "${appimage} is not a type 2 AppImage"
echo "${appimage}: the AppImage checks passed"
