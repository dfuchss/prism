## Download

| Machine | File |
|---|---|
| Mac with Apple silicon (macOS 14 or later) | [`prism-macos-arm64.zip`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-macos-arm64.zip) |
| Mac with Intel (macOS 14 or later) | [`prism-macos-x86_64.zip`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-macos-x86_64.zip) |
| Linux, 64-bit Intel/AMD (glibc 2.39 or later) | [`prism-linux-x86_64.AppImage`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-linux-x86_64.AppImage), or the small [`prism-linux-x86_64`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-linux-x86_64) that uses your system's libraries |
| Linux, 64-bit ARM (glibc 2.39 or later) | [`prism-linux-aarch64.AppImage`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-linux-aarch64.AppImage), or the small [`prism-linux-aarch64`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-linux-aarch64) that uses your system's libraries |
| Windows 10 and 11, 64-bit Intel/AMD | [`prism-windows-x86_64.exe`](https://github.com/dfuchss/prism/releases/download/{{tag}}/prism-windows-x86_64.exe) |

[`SHA256SUMS`](https://github.com/dfuchss/prism/releases/download/{{tag}}/SHA256SUMS) holds the checksums of all files. PRism does not update itself; download a newer release to update.

## Install

### macOS

> **These builds are not signed** with an Apple code-signing certificate, so macOS blocks the first start of
> PRism. You only need to do this once.

1. Unzip the file and move **PRism.app** into your **Applications** folder.
2. Start PRism. macOS blocks it. Click **Done**.
3. Open **System Settings** > **Privacy & Security**, scroll to **Security**, click **Open Anyway** next to the note about PRism and confirm with your login password.

If macOS says PRism is "damaged and can't be opened", clear the quarantine flag once: `xattr -cr /Applications/PRism.app`.
Checksums: `shasum -a 256 -c SHA256SUMS`.

### Linux

The AppImage brings its own WebKitGTK. Make it executable and start it:

```bash
chmod +x prism-linux-x86_64.AppImage
./prism-linux-x86_64.AppImage
```

AppImages need FUSE 2 (`libfuse2` on Debian/Ubuntu, `fuse` on Fedora); without it, start PRism with
`--appimage-extract-and-run`. Checksums: `sha256sum -c SHA256SUMS --ignore-missing`.

The plain binary (`prism-linux-x86_64` or `prism-linux-aarch64`) is much smaller but needs WebKitGTK 4.1 and GTK 3
from your distribution (Debian/Ubuntu: `libwebkit2gtk-4.1-0`, `libgtk-3-0`):

```bash
install -Dm755 prism-linux-x86_64 ~/.local/bin/prism
prism
```

### Windows

PRism needs the WebView2 Runtime, which Windows 11 and current Windows 10 include. It is a single program file
without an installer: move it to a folder of your own and start it from there.

> **This build is not signed**, so Windows SmartScreen warns on the first start of a downloaded copy
> ("Windows protected your PC"). Click **More info**, then **Run anyway**.

Checksums: `sha256sum -c SHA256SUMS --ignore-missing` in Git Bash, or compare with
`Get-FileHash prism-windows-x86_64.exe` in PowerShell.
