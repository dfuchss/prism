# Screenshots

`scripts/screenshots.sh` takes the screenshots in `docs/screenshots/` (used by the README and this wiki) with demo data only, on macOS:

```sh
scripts/screenshots.sh                   # macOS: docs/screenshots/*.png
```

It builds a debug bundle with the bundle id `org.fuchss.prism.demo` in `target/screenshots` (`PRISM_SHOTS_TARGET_DIR`; `PRISM_SHOTS_SKIP_BUILD=1` reuses it), starts it once per screenshot in [demo mode](Debug-Options) with a temporary config directory, captures only its window (`screencapture -l`), scales the images to 1600 px width with ImageMagick (and `pngquant` when installed), and deletes the demo's data at the end. The terminal needs the Screen Recording permission (System Settings > Privacy & Security > Screen Recording).
