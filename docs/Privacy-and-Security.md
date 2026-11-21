# Privacy and security

- **Local only:** PRism has no telemetry and no server. Nothing leaves your computer except the calls to the GitHub API (`api.github.com`) made with your own token.
- **Token handling:** PRism reads `GH_TOKEN`, then `GITHUB_TOKEN`, then the output of `gh auth token` (see [Settings](Settings#authentication)). The token is kept in memory only and is never written to disk, the audit log or any other log; PRism never asks for a password.
- **Links:** PRism opens only `https://github.com/` URLs in your browser.
- **Head-pinned actions:** approve, merge, approve-and-merge, auto-merge and rebase re-fetch the pull request first and are bound to the head commit you saw. If new commits arrived, the action is skipped ("new commits since you looked") and GitHub itself refuses an approval or merge whose head moved. See the [safety model](Rules#safety-model).
- **Suggestions by default:** a rule only suggests actions until you set `auto = true`. Automatic merges run only for patch and minor updates, never for drafts and only with green CI, and not on pull requests that are only in the inbox through the repository scope unless the rule says so.
- **One run at a time:** actions never overlap, so each run sees what the previous one changed.

## What is stored where

PRism keeps four files in its [config directory](Settings#data-locations) and nothing else:

| File | Content |
|---|---|
| `rules.toml` | The automation rules. |
| `settings.toml` | The repository scope and the `gh` path. |
| `audit.jsonl` | The audit log. |
| `auto-memo.json` | What automatic rules already did. |

## Audit log

Every executed, failed or skipped action, manual or automatic, is appended to `audit.jsonl` with time, rule (empty for manual actions), pull request, action, result and message. The Audit tab shows it. At 1 MB the file is renamed to `audit.jsonl.1` (replacing the previous one) and a new file starts; the Audit tab reads both.

## Auto memory

What automatic rules did is remembered per pull request, head commit and action (comments without the head commit, so a new push does not repeat a comment) in `auto-memo.json`, so a poll or a restart does not approve or comment twice. The newest 2000 entries are kept. Failures and skip messages are remembered for the session only, so failures are not retried during the same session. Delete the file to let the rules act again.
