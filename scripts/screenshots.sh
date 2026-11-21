#!/usr/bin/env bash
# Takes the README screenshots (docs/screenshots/*.png) with demo data only.
#
# Builds a debug bundle with its own bundle id (org.fuchss.prism.demo) in a separate target
# folder and starts it in demo mode (PRISM_DEMO=1, see DEVELOPMENT.md): a fictional inbox, no
# token, no gh, no network, and a throwaway config folder. The regular PRism installation, its
# config folder and a running debug build are not touched. Each screenshot starts the demo anew
# on one screen (PRISM_DEMO_VIEW) and captures only its window. Everything of the demo id is
# deleted at the end.
#
# Needs macOS, cargo tauri, trunk, magick (ImageMagick), optionally pngquant, and Screen
# Recording permission for the terminal.
#
# Environment:
#   PRISM_SHOTS_TARGET_DIR       cargo target folder for the demo bundle (default
#                                target/screenshots, so target/debug is not replaced)
#   PRISM_SHOTS_SKIP_BUILD       1 = reuse the bundle built by an earlier run
#   PRISM_SHOTS_WINDOW_POSITION  window position "x,y" (default 24,48 on the main display)
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$REPO/docs/screenshots"
TARGET_DIR="${PRISM_SHOTS_TARGET_DIR:-$REPO/target/screenshots}"
BUNDLE="$TARGET_DIR/debug/bundle/macos/PRism.app"
APP_BIN="$BUNDLE/Contents/MacOS/prism"
PLIST="$BUNDLE/Contents/Info.plist"
# Bundle id of the demo build; never the regular org.fuchss.prism.
DEMO_ID="org.fuchss.prism.demo"
# The demo window is 1280x800 logical (Retina: 2x); images are scaled down to this width.
MAX_WIDTH=1600
WINDOW_POSITION="${PRISM_SHOTS_WINDOW_POSITION:-24,48}"

WORK="$(mktemp -d /tmp/prism-demo.XXXXXX)"
CONFIG="$WORK/config"
DEMO_PID=""

log() { printf '\033[1m==> %s\033[0m\n' "$*"; }

stop_demo() {
    if [[ -n "$DEMO_PID" ]] && kill -0 "$DEMO_PID" 2>/dev/null; then
        kill "$DEMO_PID" 2>/dev/null || true
        for _ in $(seq 1 20); do
            kill -0 "$DEMO_PID" 2>/dev/null || break
            sleep 0.25
        done
        kill -9 "$DEMO_PID" 2>/dev/null || true
        wait "$DEMO_PID" 2>/dev/null || true
    fi
    DEMO_PID=""
}

# Deletes what WebKit and macOS store under the demo bundle id.
remove_demo_data() {
    [[ "$DEMO_ID" == org.fuchss.prism.demo ]] || return 1
    local lib="$HOME/Library"
    rm -rf "$lib/Application Support/$DEMO_ID" "$lib/WebKit/$DEMO_ID" "$lib/Logs/$DEMO_ID" \
        "$lib/Caches/$DEMO_ID" "$lib/HTTPStorages/$DEMO_ID" "$lib/HTTPStorages/$DEMO_ID.binarycookies" \
        "$lib/Saved Application State/$DEMO_ID.savedState"
    rm -f "$lib/Preferences/$DEMO_ID.plist"
}

cleanup() {
    set +e
    log "Cleaning up"
    stop_demo
    remove_demo_data
    rm -rf "$WORK"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------------------------
bundle_id() { /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST" 2>/dev/null || true; }

log "Building the demo bundle $DEMO_ID ($TARGET_DIR)"
if [[ "${PRISM_SHOTS_SKIP_BUILD:-0}" != 1 || ! -x "$APP_BIN" || "$(bundle_id)" != "$DEMO_ID" ]]; then
    (cd "$REPO/app/src-tauri" &&
        CARGO_TARGET_DIR="$TARGET_DIR" cargo tauri build --debug --bundles app \
            --config "{\"identifier\":\"$DEMO_ID\"}")
fi
[[ "$(bundle_id)" == "$DEMO_ID" ]] || { echo "demo bundle does not have the id $DEMO_ID" >&2; exit 1; }
# Leftovers of an aborted run.
remove_demo_data

# ---------------------------------------------------------------------------------------------
start_demo() { # $1: PRISM_DEMO_VIEW, $2: PRISM_DEMO_THEME
    PRISM_DEMO=1 \
        PRISM_CONFIG_DIR="$CONFIG" \
        PRISM_DEMO_VIEW="$1" \
        PRISM_DEMO_THEME="$2" \
        PRISM_DEMO_WINDOW_POSITION="$WINDOW_POSITION" \
        GH_TOKEN="" GITHUB_TOKEN="" \
        "$APP_BIN" >>"$WORK/demo.out" 2>&1 &
    DEMO_PID=$!
}

# CGWindowID of the largest on-screen window of the demo process (never the user's instance).
window_id() {
    osascript -l JavaScript - "$DEMO_PID" <<'JS'
ObjC.import("CoreGraphics");
function run(argv) {
  const pid = Number(argv[0]);
  const list = ObjC.deepUnwrap(ObjC.castRefToObject(
    $.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, $.kCGNullWindowID)));
  let best = null, area = 0;
  for (const w of list) {
    if (w.kCGWindowOwnerPID !== pid) continue;
    const a = w.kCGWindowBounds.Width * w.kCGWindowBounds.Height;
    if (a > area) { area = a; best = w.kCGWindowNumber; }
  }
  return best === null ? "" : String(best);
}
JS
}

capture() { # $1: output name, $2: demo view, $3: seconds to wait after the window appeared, $4: theme
    local name="$1" view="$2" wait="$3" theme="${4:-light}" wid=""
    log "Capturing $name.png (view $view, $theme, ${wait}s)"
    start_demo "$view" "$theme"
    for _ in $(seq 1 60); do
        wid="$(window_id)"
        [[ -n "$wid" ]] && break
        sleep 0.5
    done
    [[ -n "$wid" ]] || { echo "demo window not found; output:" >&2; cat "$WORK/demo.out" >&2; exit 1; }
    sleep "$wait"
    wid="$(window_id)"
    if ! screencapture -x -o -l"$wid" "$WORK/$name.png" || [[ ! -s "$WORK/$name.png" ]]; then
        echo "screencapture failed: allow your terminal under System Settings > Privacy & Security > Screen Recording" >&2
        exit 1
    fi
    echo "captured window $wid: $(magick identify -format "%wx%h" "$WORK/$name.png") px"
    magick "$WORK/$name.png" -resize "${MAX_WIDTH}x>" -strip "$OUT/$name.png"
    if command -v pngquant >/dev/null; then
        pngquant --force --skip-if-larger --quality 80-95 --output "$OUT/$name.png" "$OUT/$name.png" || true
    fi
    stop_demo
}

mkdir -p "$OUT"
capture inbox inbox 4
capture planned planned 3
capture rules rules 3
capture settings settings 3
capture about about 3
capture dark inbox 4 dark
# Last: the batch writes audit lines and changes nothing else that later shots would show.
capture bulk bulk 4

log "Done: $OUT"
