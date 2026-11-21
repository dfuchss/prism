# Changelog

All notable changes to PRism are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/) (`X.Y.Z`); a release is tagged `vX.Y.Z`.

## [Unreleased]

## [0.1.0] - 2026-11-21

First release.

### Added

**Inbox and buckets**

- One desktop inbox of the open pull requests you are involved in: those that notify you, those that wait for your review and your own
- Buckets Mine, Ready, Needs approval, Failing, Pending and Draft, with counts; filters by repository (grouped by owner in the sidebar), text, unread, bots and humans
- Detail pane with merge state, review and check states and the failed jobs of failing runs
- Dependency bump detection (patch, minor, major, unknown) and flagging of superseded duplicate bumps
- Keyboard navigation: `j`/`k`, Space or `x`, `o` or Enter, Esc
- Refresh at startup and then at the interval GitHub asks for; when the rate limit is reached the error says so and the background refresh waits

**Actions and bulk**

- The same actions on the selected rows (bulk bar) and on the open pull request (detail pane): approve, merge, approve and merge, auto-merge, rebase, re-run failed jobs, comment, close with an optional comment, mark read or done, unsubscribe
- Actions GitHub would refuse (own pull request, no write access, auto-merge off, archived repository, merged or closed pull request) stay visible but disabled, with the reason as tooltip; the bulk bar lists skipped pull requests in the confirm dialog
- Selection survives search and filter changes; only selected pull requests that are shown are acted on
- Progress panel for batches, clear error messages (GitHub's own message, plain reasons for refused actions)

**Rebase**

- The `rebase` action: `@dependabot rebase` for Dependabot, the rebase checkbox for Renovate, GitHub's branch update (rebase) bound to the head you saw for everyone else

**Rules and automation**

- Rules in `rules.toml`, edited in the app, with filters `authors`, `repos`, `title`, `update`, `checks`, `state`, `duplicate`, `pr_state` and `involved`, and the actions approve, merge, approve-and-merge, auto-merge, close, comment, mark-read, mark-done, unsubscribe, rerun-failed and rebase
- Planned tab with the actions the rules suggest, to review and apply
- `auto` rules that run on every refresh and after every batch
- Safety model: rules only suggest by default; a fresh re-check of the head commit and CI before approve and merge; the head you saw must match; auto rules never merge major or unknown updates, drafts or pull requests without green checks, and never act on pull requests that are only in the inbox through the repository scope unless `involved = false`; a persisted memory of automatic actions (`auto-memo.json`) so nothing repeats after a restart
- Unknown keys, duplicate rule names and `repos` globs that never match are reported; unknown keys are errors
- Audit log of all actions in `audit.jsonl` (rotated at 1 MB), shown on the Audit tab
- `examples/rules/` with tested, copy-paste-ready rules and settings

**Scope and settings**

- `[scope] include` adds all open pull requests of matching accessible repositories to the inbox (marked "via repo", hidden by "Only involving me"); `exclude` hides repositories from the whole inbox, whatever brought their pull requests in, and rules never see them
- Settings tab with a preview of the repositories in scope; saving filters the inbox at once
- `[gh] path` to set the `gh` executable
- Sign-in through `GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token`; the token stays in memory

**Notifications and merged handling**

- Merged and closed pull requests that still have a notification are kept under "Merged/Closed" with a state badge and offer only Mark read, Mark done and Unsubscribe; mark done and unsubscribe update the row immediately
- Rule filter `pr_state`: rules act on open pull requests only unless they say otherwise

**Platforms and packaging**

- macOS 14+ (Apple silicon and Intel), Linux (x86_64 and aarch64, AppImage or plain binary) and Windows 10/11 (portable executable); builds are unsigned and PRism does not update itself
- Release workflow for version tags with `SHA256SUMS`

**About**

- About tab (and "About PRism" in the macOS menu) with version, platform, config directory, GitHub sign-in status, links and the GPL notice
- Logo, light and dark theme

[Unreleased]: https://github.com/dfuchss/prism/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dfuchss/prism/releases/tag/v0.1.0
