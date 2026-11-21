# Rules

Rules live in `rules.toml` in the [config directory](Settings#data-locations). PRism creates it with a commented header the first time it starts (see the examples below). The Rules tab edits and saves it; PRism reports invalid TOML, unknown keys or unknown actions when you save, and warns about rules without actions, rules sharing a name, invalid title regexes and `repos` globs that never match. If the file is invalid at startup, no rules are active and the file is left untouched.

By default a rule only suggests actions on the [Planned tab](Usage#planned). A rule with `auto = true` runs on every refresh, under the [safety model](#safety-model) below.

## Fields

A rule is a `[[rule]]` table with a unique `name`. Every filter is optional; a filter left out matches everything (except `checks`, which defaults to `green`, and `pr_state`, which defaults to open only), and a rule matches only when all its filters hold. Unknown keys are errors, so a misspelled filter cannot silently widen a rule. Draft pull requests never match.

| Field | Meaning |
|---|---|
| `name` | Required. Names the rule in the Planned tab and the audit log; keep names unique. |
| `authors` | Exact logins, compared case-insensitively, for example `["renovate[bot]", "dependabot[bot]"]`. |
| `repos` | Globs over `owner/name`, case-insensitive. `*` matches any characters within one part, `?` one character, and a part that is exactly `**` any number of parts: `"acme/*"`, `"*/widget"`, `"acme/**"`. |
| `title` | A regular expression ([Rust regex syntax](https://docs.rs/regex/latest/regex/#syntax)) searched in the title. An invalid regex never matches. |
| `update` | A list of `patch`, `minor`, `major`, `unknown`. Parsed from titles like "from X to Y" or "to vY"; a title that is no version bump is `unknown`. |
| `checks` | `green` (default): all checks pass, or there are no checks and GitHub reports the pull request as clean. `any`: no requirement. |
| `state` | A list of GitHub merge states the pull request must be in: `clean`, `blocked`, `unstable`, `dirty` (conflicts), `behind`, `draft`, `has-hooks`, `unknown`. Empty (default): any. Draft pull requests never match, so `draft` is of little use. |
| `duplicate` | `true`: only superseded duplicates; `false`: only pull requests that are not. |
| `pr_state` | A list of `open`, `merged`, `closed`. Empty (default): `["open"]`, so rules never act on merged or closed pull requests unless they say so. Merged and closed pull requests are only seen while they have a notification, and only `mark-read`, `mark-done` and `unsubscribe` ever run on them. `mark-read` is not planned for an already read notification, `mark-done` and `unsubscribe` not without a notification. |
| `involved` | `true`: only pull requests that involve you (a notification, a review request from you, or your own); `false`: only pull requests that are in the inbox through the [repository scope](Settings#repository-scope). Use `involved = true` to keep an `auto` rule away from other people's pull requests in scope repositories. |
| `actions` | A list of action strings, run in order; see below. |
| `auto` | `false` (default): the rule only suggests. `true`: it runs on every refresh. |

## Actions

| Action | Effect |
|---|---|
| `approve` | Approve the head commit; does nothing if you already approved it. |
| `merge[:method]` | Merge. The method is `squash`, `merge` or `rebase` and must be allowed by the repository; without it, the repository's first allowed method is used. |
| `approve-and-merge[:method]` | Approve, then merge. |
| `auto-merge[:method]` | Enable GitHub auto-merge. |
| `close[:"text"]` | Close the pull request, with an optional comment. |
| `comment:"text"` | Add a comment. |
| `mark-read` | Mark the notification as read. |
| `mark-done` | Mark the notification as done. |
| `unsubscribe` | Unsubscribe from the thread. |
| `rerun-failed` | Re-run the failed jobs of the failing workflow runs. |
| `rebase` | Bring the branch up to date; see [below](#rebase). |

Quotes around texts are optional. The notification actions need a notification thread for the pull request. If several rules plan the same action for the same pull request, it is planned once, by the first rule.

### Rebase

The `rebase` action (also the Rebase button in the detail pane and the bulk bar) works by the author of the pull request:

- `dependabot[bot]`: PRism comments `@dependabot rebase`.
- `renovate[bot]`: PRism fetches the current description, ticks the `- [ ] <!-- rebase-check -->` checkbox and saves the description. If the box is missing or already ticked, the action fails with a message ("rebase already requested").
- Anyone else: GitHub's `updatePullRequestBranch` with the rebase method, bound to the head commit you saw.

Like approve and merge, it re-fetches the pull request first and is skipped if the head moved since you looked. Auto rules run it at most once per pull request and head commit and never on drafts; it never merges. Example (a pull request with conflicts usually has failing CI, so this rule needs `checks = "any"`; the default `green` would skip it):

```toml
[[rule]]
name = "rebase stale bot PRs"
authors = ["dependabot[bot]", "renovate[bot]"]
state = ["behind", "dirty"]
checks = "any"
actions = ["rebase"]
auto = true
```

## Safety model

PRism is careful because approving and merging cannot be undone from the app.

- **Suggestions by default.** A rule with `auto = false` (the default) only appears on the Planned tab. Nothing runs until you apply it, after a confirmation.
- **Fresh re-check.** Before approve, merge, approve-and-merge and auto-merge, PRism fetches the pull request again and checks the head commit and CI as they are now, not as they were at the last refresh. Approvals and merges are bound to that head commit, so GitHub refuses them if it moved.
- **The head you saw must match.** If new commits arrived since the row, the detail pane or the plan was shown, the action is skipped with "new commits since you looked". Refresh and retry.
- **Applied suggestions are re-checked.** The rule must still exist and still match the fresh state.
- **Auto rules** (`auto = true`) run on every refresh against fresh data. The rule must still match, and a merging action (`merge`, `approve-and-merge`, `auto-merge`) runs only for patch and minor updates, never for major or unknown ones, never for drafts, and only when CI is strictly green (no checks at all is not enough). `rebase` is allowed (never for drafts); other actions are not restricted, except on merged and closed pull requests, where only `mark-read`, `mark-done` and `unsubscribe` are allowed (and need no fresh re-check). A skipped or failed action skips the later actions for that pull request.
- **Auto rules stay with your pull requests.** An `auto = true` rule never acts on a pull request that is only in the inbox through the [repository scope](Settings#repository-scope), unless the rule says `involved = false`. Otherwise one auto merge rule would act in every repository of the include list. Suggestions on the Planned tab still show all matches.
- **Auto memory.** What auto rules did is remembered (per pull request, head commit and action; comments without the head commit, so a new push does not repeat a comment) and persisted in `auto-memo.json`, so a poll or a restart does not approve or comment twice. The newest 2000 entries are kept. Failures are not retried during the same session.
- **One run at a time.** Actions never overlap, so each run sees what the previous one changed.
- **Audit log.** Every executed, failed or skipped action is appended to `audit.jsonl` with time, rule (empty for manual actions), pull request, action, result and message, and shown on the Audit tab.
- **Links.** PRism opens only `https://github.com/` URLs in your browser.

## Examples

The default `rules.toml` is only a commented header with one example: no rule is active until you add one. Ready-to-copy rules live in [`examples/rules/`](https://github.com/<<REPO>>/tree/main/examples/rules), each a complete, commented file that is tested to parse and validate:

- [`approve-green-bot-prs.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/approve-green-bot-prs.toml): suggest approving green Dependabot/Renovate pull requests.
- [`merge-green-patch-minor.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/merge-green-patch-minor.toml): approve and merge green patch/minor bot bumps automatically.
- [`rebase-stale-bot-prs.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/rebase-stale-bot-prs.toml): rebase bot pull requests that are behind or conflicting.
- [`close-duplicates.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/close-duplicates.toml): close superseded duplicates with a comment.
- [`mark-merged-read.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/mark-merged-read.toml): mark notifications of merged pull requests as read.
- [`scope-only-safety.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/scope-only-safety.toml): `involved = true` / `false`.
- [`settings.example.toml`](https://github.com/<<REPO>>/blob/main/examples/rules/settings.example.toml): `[scope]` and `[gh]`, see [Settings](Settings).
