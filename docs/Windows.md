# PRism on Windows

PRism runs on Windows 10 and 11 (x86_64) and uses the WebView2 Runtime, which Windows 11 and current Windows 10 include.

## Install

Download `prism-windows-x86_64.exe` from the [latest release](https://github.com/<<REPO>>/releases/latest). It is a single portable program without an installer: move it to a folder of your own and start it from there. `SHA256SUMS` holds the checksums (`sha256sum -c SHA256SUMS --ignore-missing` in Git Bash, or compare with `Get-FileHash prism-windows-x86_64.exe` in PowerShell).

The build is not signed, so Windows SmartScreen warns on the first start of a downloaded copy ("Windows protected your PC"). Click **More info**, then **Run anyway**.

## First start

Sign in with the GitHub CLI: `gh auth login`. PRism starts `gh` without a console window. If PRism cannot find `gh`, set `[gh] path` in the Settings tab; see [Settings](Settings#the-gh-path).

## Data locations

`%APPDATA%\org.fuchss.prism\`: `rules.toml`, `settings.toml`, `audit.jsonl` and `auto-memo.json`; see [Settings](Settings#data-locations).

## Known limitations

- The build is not signed (see above), and there is no installer or updater: replace the file to update.
- Without the WebView2 Runtime PRism does not start; install it from Microsoft.
