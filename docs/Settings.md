# Settings

`settings.toml` in the [config directory](#data-locations) holds the repository scope and the path of the `gh` executable. The Settings tab edits and saves it; PRism creates the file with an empty scope, which changes nothing. An invalid `settings.toml` is shown in the error banner and the scope is empty until it is fixed.

## Repository scope

By default the inbox holds the pull requests you are involved in. With a scope, PRism also adds every open pull request of the repositories you choose.

```toml
[scope]
# Open PRs of every repo you can access that matches one of these globs are added to the inbox.
include = ["example-org/*"]
# Optional. Repos matching these globs are hidden from the whole inbox: notifications, review
# requests and your own PRs too (rules never see them). Wins over include.
exclude = []
```

- `include` only adds repositories. An empty `include` adds nothing.
- `exclude` hides a repository from everything, not only from the scope: its pull requests disappear from the inbox even when they come from notifications, review requests or your own authorship, so rules (auto rules included) never see them. It wins over `include`.
- Globs work like the `repos` globs of [rules](Rules#fields) (`owner/name`, case-insensitive, `*`, `?`, `**`). A pattern without a `/` never matches and is reported as a warning when you save.
- PRism lists the non-archived repositories you own, collaborate on or can reach as an organization member, and keeps the list for an hour (a save of `settings.toml` fetches it again). **Preview scope** shows the repositories the current text covers and the accessible ones it excludes. Saving applies the change to the inbox immediately; the newly covered repositories are fetched right afterwards.
- Pull requests that are only in the inbox because of the scope have no notification, no review request from you and are not yours. They show "via repo" in the Why column, and the sidebar toggle "Only involving me" hides them.
- If the scope cannot be fetched, the banner says so and the rest of the inbox is unaffected.
- Rules also see these pull requests. Auto rules ignore them unless the rule sets `involved = false`; suggestions show them.

## Authentication

PRism uses a GitHub token and never asks for a password. It reads, in this order:

1. the environment variable `GH_TOKEN`,
2. the environment variable `GITHUB_TOKEN`,
3. the output of `gh auth token` (the [GitHub CLI](https://cli.github.com/); run `gh auth login` once).

The token is kept in memory only; PRism does not write it to disk or to the logs. When no token is available or GitHub rejects it, the inbox shows a banner; fix the sign-in and press Refresh. The token must be allowed to read notifications and to read and write pull requests of the repositories you act on. The About tab shows the token source.

Environment variables only reach PRism when it is started from a shell that has them; when you open the app from the Finder, use `gh auth login`.

### The `gh` path

`gh` is looked up on `PATH`. An app opened from the Finder often has a short `PATH` that lacks Homebrew, so set the executable explicitly:

```toml
[gh]
path = "/opt/homebrew/bin/gh"   # empty (default) = "gh" from PATH
```

PRism does not search install locations. Saving a path that does not exist or is not executable is refused. A saved change takes effect at once: the cached token is dropped and the inbox refreshes. If `gh` cannot be started, the banner names the path PRism tried and tells you to set `[gh] path` or run `gh auth login`. `GH_TOKEN` and `GITHUB_TOKEN` still take precedence.

## Data locations

PRism keeps its files in its config directory:

| OS | Directory |
|---|---|
| macOS | `~/Library/Application Support/org.fuchss.prism/` |
| Linux | `~/.config/org.fuchss.prism/` (or `$XDG_CONFIG_HOME/org.fuchss.prism/`) |
| Windows | `%APPDATA%\org.fuchss.prism\` |

The About tab shows the exact path.

| File | Content |
|---|---|
| `rules.toml` | The [automation rules](Rules). |
| `settings.toml` | The repository scope and the `gh` path. |
| `audit.jsonl` | The [audit log](Privacy-and-Security#audit-log), one JSON object per line. At 1 MB it is renamed to `audit.jsonl.1` (replacing the previous one) and a new file starts; the Audit tab reads both. |
| `auto-memo.json` | What auto rules already did. Delete it to let them act again. |

There are no other files; the token is never stored.
