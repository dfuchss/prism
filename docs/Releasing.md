# Releasing

Releases are built by `.github/workflows/release.yml` when a tag `v<version>` is pushed; the tag must match the version in the workspace `Cargo.toml`, the only place the version is set (`tauri.conf.json` inherits it and must not set its own). The workflow checks that, runs `scripts/check.sh`, builds unsigned artifacts, writes `SHA256SUMS` and publishes a GitHub Release whose notes are the CHANGELOG section plus `.github/release-install.md`.

| Artifact | Built on |
|---|---|
| `prism-macos-arm64.zip`, `prism-macos-x86_64.zip` (zipped `PRism.app`, macOS 14+) | `macos-latest`, `--bundles app` |
| `prism-linux-x86_64.AppImage`, `prism-linux-aarch64.AppImage` and the plain binaries `prism-linux-x86_64`, `prism-linux-aarch64` | `ubuntu-24.04`, `ubuntu-24.04-arm` (the oldest glibc supported) |
| `prism-windows-x86_64.exe` (portable, no installer) | `windows-latest`, `--no-bundle` |
| `SHA256SUMS` | all of the above |

## Changelog

Notes for the user-visible changes go under `## [Unreleased]` in [`CHANGELOG.md`](https://github.com/<<REPO>>/blob/main/CHANGELOG.md) as they happen, in the format of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/): sections *Added*, *Changed*, *Fixed*, *Security*; leave out what is empty. Versions follow [Semantic Versioning](https://semver.org/).

## Release steps

1. Set `version` in the workspace `Cargo.toml` (`tauri.conf.json` has no version of its own and inherits it) and let `Cargo.lock` follow (`cargo metadata`).
2. Move the `## [Unreleased]` entries of `CHANGELOG.md` under a new `## [X.Y.Z] - YYYY-MM-DD` heading, leave an empty `## [Unreleased]` above it and update the link references at the bottom (`.github/scripts/release-notes.sh` extracts the section up to the link references).
3. Run `scripts/check.sh`, commit and push to `main`.
4. Create a signed tag and push it: `git tag -s vX.Y.Z -m "vX.Y.Z"`, `git push origin vX.Y.Z`. The tag triggers the release workflow.
5. Check the published GitHub Release: artifacts, `SHA256SUMS` and the notes.

## Notes

The Linux bundle config is `tauri.linux.conf.json` (AppImage only); the plain binary needs WebKitGTK 4.1 from the system. See [Building](Building#notes) for the build pitfalls.

## Wiki

The pages of this wiki live in `docs/` of the repository; `.github/workflows/docs.yml` mirrors them into the wiki on every push to `main` that changes `docs/`. Edit them there, not in the wiki: the next run replaces the wiki's content. Links to the repository use the placeholder `<<REPO>>`, which the workflow replaces with the repository name. The workflow needs the repository secret `PAT_TOKEN` (a personal access token that can push to the wiki), and the wiki must be initialised once by creating a first page in the GitHub UI.
