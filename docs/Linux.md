# PRism on Linux

PRism runs on Linux with WebKitGTK 4.1 and GTK 3, on x86_64 and aarch64.

## Install

PRism comes in two forms: the AppImage, which brings its own libraries, and a plain binary, much smaller, which uses the libraries of your system. Download one for your machine from the [latest release](https://github.com/<<REPO>>/releases/latest):

| Machine | AppImage | Binary |
|---|---|---|
| 64-bit Intel/AMD | `prism-linux-x86_64.AppImage` | `prism-linux-x86_64` |
| 64-bit ARM | `prism-linux-aarch64.AppImage` | `prism-linux-aarch64` |

`SHA256SUMS` holds the checksums (`sha256sum -c SHA256SUMS --ignore-missing`). Both forms are built on Ubuntu 24.04 and need glibc 2.39 or newer.

### AppImage

The AppImage brings its own WebKitGTK. Make it executable and start it:

```sh
chmod +x prism-linux-x86_64.AppImage
./prism-linux-x86_64.AppImage
```

AppImages need FUSE 2 (`libfuse2` on Debian/Ubuntu, `fuse` on Fedora); without it, start PRism with `--appimage-extract-and-run`.

### Binary

The binary needs WebKitGTK 4.1 and GTK 3 from your distribution (Debian/Ubuntu: `libwebkit2gtk-4.1-0`, `libgtk-3-0`). Make the file executable and put it in a folder of your own, for example `~/.local/bin`:

```sh
install -Dm755 prism-linux-x86_64 ~/.local/bin/prism
prism
```

## First start

Sign in with the GitHub CLI: `gh auth login`. If PRism is started from a launcher with a short `PATH`, set `[gh] path` in the Settings tab; see [Settings](Settings#the-gh-path).

## Data locations

`~/.config/org.fuchss.prism/` (or `$XDG_CONFIG_HOME/org.fuchss.prism/`): `rules.toml`, `settings.toml`, `audit.jsonl` and `auto-memo.json`; see [Settings](Settings#data-locations).

## Known limitations

- The AppImage needs FUSE 2; the binary needs a distribution with WebKitGTK 4.1.
- The builds are not signed; verify them with `SHA256SUMS`.
