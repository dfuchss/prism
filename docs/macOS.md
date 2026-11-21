# PRism on macOS

PRism runs on macOS 14 or later, on Apple silicon and Intel, and uses the system's WebKit.

## Install

Download `prism-macos-arm64.zip` (Apple silicon) or `prism-macos-x86_64.zip` (Intel) from the [latest release](https://github.com/<<REPO>>/releases/latest). `SHA256SUMS` holds the checksums (`shasum -a 256 -c SHA256SUMS`).

1. Unzip the file and move **PRism.app** into your **Applications** folder.
2. Start PRism. The app is not signed, so macOS blocks the first start. Click **Done**.
3. Open **System Settings** > **Privacy & Security**, scroll to **Security**, click **Open Anyway** next to the note about PRism and confirm with your login password. (Or right-click the app, choose Open, then Open.)

If macOS says PRism is "damaged and can't be opened", clear the quarantine flag once: `xattr -cr /Applications/PRism.app`. You only need to do this once.

## First start

Sign in with the GitHub CLI: `gh auth login`. An app opened from the Finder has a short `PATH` that lacks Homebrew, so set `[gh] path` (for example `/opt/homebrew/bin/gh`) in the Settings tab if PRism cannot find `gh`; see [Settings](Settings#the-gh-path). `GH_TOKEN` and `GITHUB_TOKEN` only reach PRism when it is started from a shell that has them. The About page also opens from the app menu ("About PRism").

## Data locations

`~/Library/Application Support/org.fuchss.prism/`: `rules.toml`, `settings.toml`, `audit.jsonl` and `auto-memo.json`; see [Settings](Settings#data-locations).

## Known limitations

- The build is not signed (see above).
- Release builds need macOS 14 or later.
