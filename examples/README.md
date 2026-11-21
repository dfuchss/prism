# Examples

Copy-paste-ready fragments. Each file is a complete, commented `rules.toml` (or `settings.toml`); paste the `[[rule]]` tables you want into your own file on the Rules tab. They are tested to parse and validate.

- `rules/approve-green-bot-prs.toml`: suggest approving green Dependabot/Renovate pull requests.
- `rules/merge-green-patch-minor.toml`: approve and merge green patch/minor bot bumps automatically.
- `rules/rebase-stale-bot-prs.toml`: rebase bot pull requests that are behind or conflicting.
- `rules/close-duplicates.toml`: close superseded duplicate bumps with a comment.
- `rules/mark-merged-read.toml`: mark notifications of merged pull requests as read.
- `rules/scope-only-safety.toml`: `involved = true` / `false` to keep auto rules away from scope-only pull requests.
- `rules/settings.example.toml`: `[scope]` and `[gh]` in `settings.toml`.
