# Usage

PRism has six tabs: Inbox, Planned, Rules, Settings, Audit and About. It refreshes at startup and then at the interval GitHub asks for, at least every 60 seconds; Refresh fetches immediately. Before the first start, sign in to GitHub: see [Settings](Settings#authentication).

## Inbox

The inbox has a sidebar on the left, a table in the middle and, when a row is open, a detail pane on the right.

### Buckets

The sidebar lists the buckets with their counts: All, Ready, Needs approval, Failing, Pending, Mine, Draft, Duplicates and Merged/Closed. Merged and closed pull requests that still have a notification are not bucketed; they are listed under "Merged/Closed" (with a state badge), where you can only mark them read or done or unsubscribe. Duplicates lists the superseded duplicates (several pull requests bumping the same dependency in the same repository; the one with the highest target version is kept).

The toggle "Only involving me" hides the pull requests that are only in the inbox because of the [repository scope](Settings#repository-scope); they show "via repo" in the Why column.

### Filters

- The search box matches title, repository, author and `#number`.
- The chips Unread, Bots and Humans narrow the table further.
- The owner sidebar lists the repositories, grouped by owner, with counts: a click on an owner filters to all its repositories, the caret expands it, a click on a repository filters to that one, and a second click removes the filter.

### The table

The columns are Repo, #, Title, Author, Why (the reason the pull request is in your inbox), CI and Age. Dependency bumps are recognised from the title ("from X to Y", "to vY"): patch, minor, major or unknown.

### Detail pane

Click a row to open its detail pane: the merge state, the review state, the files and labels, all actions, the checks and the failed jobs of failing runs. Close pane (or Esc) closes it.

### Bulk bar and More menu

Select rows with their checkboxes (or Space or `x`) and a bar appears at the bottom of the table: the number of selected pull requests (with a note when a filter hides some of them: those stay selected but are not acted on), the main actions Approve, Merge, Approve + Merge, Rebase, Close... and Mark done, a **More** menu with Auto-merge, Rerun failed, Comment..., Mark read and Unsubscribe, and a button to clear the selection. The detail pane offers all of them for the open pull request.

An action that does not apply to a pull request (it is merged, you lack the permission, ...) is disabled with the reason as tooltip; for a selection, those pull requests are skipped and the confirmation dialog says how many and why. Merge, Approve + Merge and Auto-merge ask for a confirmation, Close and Comment open a dialog for the comment. A progress panel shows each job; the results are written to the [audit log](Privacy-and-Security#audit-log). Approve and merge are checked against the head commit you saw: see the [safety model](Rules#safety-model).

### Rebase

The Rebase action (also the `rebase` action of rules) works by the author of the pull request:

- `dependabot[bot]`: PRism comments `@dependabot rebase`.
- `renovate[bot]`: PRism fetches the current description, ticks the `- [ ] <!-- rebase-check -->` checkbox and saves the description. If the box is missing or already ticked, the action fails with a message ("rebase already requested").
- Anyone else: GitHub's `updatePullRequestBranch` with the rebase method, bound to the head commit you saw.

Like approve and merge, it re-fetches the pull request first and is skipped if the head moved since you looked. It never merges.

### Keyboard shortcuts

In the inbox table:

| Key | Action |
|---|---|
| `j` or Down | Open the next row in the detail pane |
| `k` or Up | Open the previous row |
| Space or `x` | Select or deselect the open row |
| `o` or Enter | Open the pull request on GitHub |
| Esc | Close the detail pane |

The shortcuts are ignored while a checkbox or another control of the table has the focus.

## Planned

The Planned tab lists the actions your [rules](Rules) suggest: rule, pull request, title and action. Entries start checked; uncheck the ones you do not want and press **Apply selected**, then confirm. Applied suggestions are re-checked against fresh data: the rule must still exist and still match. The list reloads whenever the inbox changes.

## Rules

The Rules tab edits `rules.toml`. PRism reports invalid TOML, unknown keys and unknown actions when you save. See [Rules](Rules).

## Settings

The Settings tab edits `settings.toml`: the repository scope and the `gh` path. **Preview scope** shows the repositories the current text covers and the accessible ones it excludes. See [Settings](Settings).

## Audit

The Audit tab shows the audit log: every executed, failed or skipped action with time, rule, pull request, action, result and message. Reload re-reads the files. See [Privacy and security](Privacy-and-Security#audit-log).

## About

The About tab shows the version, the license and links, and the state of the connection: GitHub (the signed-in user), the token source, the config directory, the platform and the Tauri version. On macOS the same page opens from the app menu ("About PRism").

## Updates

PRism does not update itself. Download a newer release from the [releases](https://github.com/<<REPO>>/releases) page and replace the old file; your rules and settings stay in the [config directory](Settings#data-locations).
