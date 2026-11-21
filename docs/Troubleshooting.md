# Troubleshooting

Problems show up as a banner above the inbox, as a toast, or as a line in the [audit log](Privacy-and-Security#audit-log). The About tab shows the GitHub user, the token source and the config directory.

## "gh is not installed or not logged in"

PRism found no `GH_TOKEN`, no `GITHUB_TOKEN` and `gh auth token` gave no token. Install the [GitHub CLI](https://cli.github.com/) and run `gh auth login`, then press Refresh.

## "could not start `gh`"

PRism could not start the `gh` executable. An app opened from the Finder (macOS) or a launcher often has a short `PATH` that lacks Homebrew. Set the executable in the Settings tab:

```toml
[gh]
path = "/opt/homebrew/bin/gh"
```

See [Settings](Settings#the-gh-path). Environment variables like `GH_TOKEN` only reach PRism when it is started from a shell that has them.

## "GitHub rejected the token (HTTP 401)"

The token is expired or revoked. Run `gh auth login` or fix `GH_TOKEN`, then press Refresh. The token must be allowed to read notifications and to read and write pull requests of the repositories you act on.

## "GitHub rate limit reached"

PRism waits for the time GitHub names and then tries again; the poller never refreshes more often than every 60 seconds. Reduce the repository scope if you hit the limit regularly.

## An action is disabled or skipped

A disabled button names the reason in its tooltip: you cannot approve your own pull request, you lack write access, the pull request is merged or closed, auto-merge is disabled in the repository's settings or already enabled. In a selection, such pull requests are skipped and the confirmation says how many and why.

- **"new commits since you looked":** the head changed after the row was shown. Refresh and retry.
- **"rebase already requested":** the Renovate checkbox is missing or already ticked.
- **Merge method not allowed:** use `merge:squash`, `merge:merge` or `merge:rebase` as the repository allows, or leave the method out.

## A pull request is missing

- It is hidden by an `exclude` glob in `settings.toml`, which hides a repository from everything.
- A filter, the search or "Only involving me" hides it; merged and closed pull requests are only listed while they have a notification.
- A pull request of a repository you do not own appears only if it notifies you, wants your review, is yours, or its repository is in the scope `include`.

## A rule does nothing

Rules without `auto = true` only suggest: look at the Planned tab. Draft pull requests never match, `checks` defaults to `green`, and auto rules ignore pull requests that are only in the inbox through the scope unless `involved = false`. If an auto rule already acted, `auto-memo.json` remembers it; delete the file to let it act again. If `rules.toml` is invalid at startup, no rules are active; the Rules tab shows the error.

## Where to look

The audit log shows every action and its result. For the files, see [Data locations](Settings#data-locations); platform specifics are on [macOS](macOS), [Linux](Linux) and [Windows](Windows). Report bugs on the [issue tracker](https://github.com/<<REPO>>/issues).
