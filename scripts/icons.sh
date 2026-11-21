#!/usr/bin/env bash
# Regenerates every raster icon from the SVG sources in app/src-tauri/icons/src.
#
# Needs macOS (iconutil) and ImageMagick (`magick`, rendering SVG through Inkscape or librsvg).
# Run it after editing an SVG and commit the result.
#
#   prism.svg        free-standing prism: icon.png, 128px PNGs, .ico (64+)
#   prism-macos.svg  prism on the dark macOS squircle: icon.icns
#   prism-small.svg  simplified for 24-48 px (also composed onto the squircle for icns 32 px)
#   prism-small-16.svg  extra-simplified for the 16 px slots
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=app/src-tauri/icons/src
OUT=app/src-tauri/icons
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

render() { magick -background none -density 144 "$1" -resize "${2}x${2}" -define png:color-type=6 "$3"; }

# Small variants on the squircle for the 16/32 px icns entries.
squircle() { # $1 = source svg, $2 = output svg
  {
    echo '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">'
    echo '<defs><linearGradient id="grad-surface" gradientUnits="userSpaceOnUse" x1="0" y1="100" x2="0" y2="924">'
    echo '<stop offset="0" stop-color="#232A4D"/><stop offset="1" stop-color="#0B0F26"/></linearGradient></defs>'
    echo '<rect x="100" y="100" width="824" height="824" rx="185" fill="url(#grad-surface)"/>'
    echo '<g transform="translate(512 512) scale(0.7) translate(-515 -470)">'
    sed '1d;$d' "$1"
    echo '</g></svg>'
  } >"$2"
}
squircle "$SRC/prism-small.svg" "$TMP/macos-small.svg"
squircle "$SRC/prism-small-16.svg" "$TMP/macos-small-16.svg"

render "$SRC/prism.svg" 512 "$OUT/icon.png"
render "$SRC/prism.svg" 256 "$OUT/128x128@2x.png"
render "$SRC/prism.svg" 128 "$OUT/128x128.png"
render "$SRC/prism.svg" 64 "$OUT/64x64.png"
render "$SRC/prism-small.svg" 32 "$OUT/32x32.png"

ICONSET="$TMP/icon.iconset"
mkdir "$ICONSET"
for size in 16 32 128 256 512; do
  for scale in 1 2; do
    px=$((size * scale))
    name="icon_${size}x${size}"
    [ "$scale" = 2 ] && name="${name}@2x"
    if [ "$px" -eq 16 ]; then
      render "$TMP/macos-small-16.svg" "$px" "$ICONSET/$name.png"
    elif [ "$px" -le 32 ]; then
      render "$TMP/macos-small.svg" "$px" "$ICONSET/$name.png"
    else
      render "$SRC/prism-macos.svg" "$px" "$ICONSET/$name.png"
    fi
  done
done
iconutil -c icns "$ICONSET" -o "$OUT/icon.icns"

render "$SRC/prism-small-16.svg" 16 "$TMP/ico-16.png"
for px in 24 32 48; do render "$SRC/prism-small.svg" "$px" "$TMP/ico-$px.png"; done
for px in 64 256; do render "$SRC/prism.svg" "$px" "$TMP/ico-$px.png"; done
magick "$TMP"/ico-16.png "$TMP"/ico-24.png "$TMP"/ico-32.png "$TMP"/ico-48.png \
  "$TMP"/ico-64.png "$TMP"/ico-256.png "$OUT/icon.ico"

echo "icons regenerated"
