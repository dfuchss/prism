//! Automation rules: the schema of `rules.toml` and the pure engine
//! `evaluate(rules, items) -> Vec<PlannedAction>`.
//!
//! A rule matches a pull request when all of its filters hold (a filter left out matches
//! anything, except `checks`, which defaults to green): author, repository glob, title regex,
//! update kind, CI state, merge state (`state`), the duplicate flag, `involved` and `pr_state`
//! (empty: open only). Draft pull requests never match, and actions GitHub would refuse for a
//! pull request (`unavailable_reason`) are never planned.

mod glob;
mod rule;

pub use glob::{glob_match, glob_problem};
use protocol::{
    Action, CiSummary, MergeState, PlannedAction, PrItem, PrState, UpdateKind, unavailable_reason,
};
use regex::Regex;
pub use rule::{ChecksReq, DuplicateFilter, Involvement, Rule, RuleSet};
use std::collections::HashSet;

/// Plans the actions of every matching rule: per rule, per matching PR, one [`PlannedAction`]
/// per action in rule order. Identical `(pr, action)` pairs are planned once, by the first rule.
/// `rule_index` is the position in `rules`.
pub fn evaluate(rules: &[Rule], items: &[PrItem]) -> Vec<PlannedAction> {
    let mut planned: Vec<PlannedAction> = Vec::new();
    for (rule_index, rule) in rules.iter().enumerate() {
        let title = compile_title(rule);
        for item in items.iter().filter(|item| matches_with(rule, &title, item)) {
            for action in rule
                .actions
                .iter()
                .filter(|a| worth_planned_notification(a, item))
                .filter(|a| unavailable_reason(item, a).is_none())
            {
                let seen = planned
                    .iter()
                    .any(|p| p.pr == item.pr && p.action == *action);
                if !seen {
                    planned.push(PlannedAction {
                        rule: rule.name.clone(),
                        rule_index,
                        pr: item.pr.clone(),
                        action: action.clone(),
                        head_oid: item.head_oid.clone(),
                    });
                }
            }
        }
    }
    planned
}

/// Notification actions without anything to do are not planned: marking an already read
/// thread as read is a no-op (so an auto rule would otherwise fire on every poll), and
/// without a notification there is nothing to mark, finish or unsubscribe from.
fn worth_planned_notification(action: &Action, item: &PrItem) -> bool {
    match action {
        Action::MarkRead => item.notification.as_ref().is_some_and(|n| n.unread),
        Action::MarkDone | Action::Unsubscribe => item.notification.is_some(),
        _ => true,
    }
}

/// Whether `rule` applies to `item`. A rule with an invalid title regex never matches.
pub fn matches(rule: &Rule, item: &PrItem) -> bool {
    matches_with(rule, &compile_title(rule), item)
}

/// `Ok(None)`: no title filter; `Err`: invalid regex.
type TitleFilter = Result<Option<Regex>, regex::Error>;

fn compile_title(rule: &Rule) -> TitleFilter {
    rule.title.as_deref().map(Regex::new).transpose()
}

fn matches_with(rule: &Rule, title: &TitleFilter, item: &PrItem) -> bool {
    if item.is_draft() || !pr_state_matches(rule, item) {
        return false;
    }
    let title_ok = match title {
        Ok(Some(regex)) => regex.is_match(&item.title),
        Ok(None) => true,
        Err(_) => false,
    };
    title_ok
        && author_matches(rule, item)
        && repo_matches(rule, item)
        && kind_matches(rule, item)
        && checks_match(rule, item)
        && merge_state_matches(rule, item)
        && rule.duplicate.accepts(item.duplicate_of.is_some())
        && rule.involved.accepts(item.via_scope_only())
}

/// An empty `pr_state` means open only, so existing rules never act on merged PRs.
fn pr_state_matches(rule: &Rule, item: &PrItem) -> bool {
    if rule.pr_state.is_empty() {
        item.state == PrState::Open
    } else {
        rule.pr_state.contains(&item.state)
    }
}

fn author_matches(rule: &Rule, item: &PrItem) -> bool {
    rule.authors.is_empty()
        || rule
            .authors
            .iter()
            .any(|author| author.eq_ignore_ascii_case(&item.author))
}

fn repo_matches(rule: &Rule, item: &PrItem) -> bool {
    rule.repos.is_empty()
        || rule
            .repos
            .iter()
            .any(|pattern| glob_match(pattern, &item.pr.repo))
}

/// The bump kind of the item; `Unknown` when it is no recognized bump.
fn bump_kind(item: &PrItem) -> UpdateKind {
    item.bump
        .as_ref()
        .map_or(UpdateKind::Unknown, |bump| bump.kind)
}

fn kind_matches(rule: &Rule, item: &PrItem) -> bool {
    rule.update_kinds.is_empty() || rule.update_kinds.contains(&bump_kind(item))
}

fn merge_state_matches(rule: &Rule, item: &PrItem) -> bool {
    rule.merge_state.is_empty() || rule.merge_state.contains(&item.merge_state)
}

/// `Green` needs green CI; no checks at all only counts when GitHub says the PR is `Clean`.
fn checks_match(rule: &Rule, item: &PrItem) -> bool {
    match rule.checks {
        ChecksReq::Any => true,
        ChecksReq::Green => match item.ci() {
            CiSummary::Green => true,
            CiSummary::None => item.merge_state == MergeState::Clean,
            CiSummary::Failing | CiSummary::Pending => false,
        },
    }
}

/// Safety gate for `auto = true` rules. Items that are only in the inbox through the repository
/// scope are refused unless the rule says `involved = false`. Merging actions (`Merge`,
/// `ApproveAndMerge`, `EnableAutoMerge`) are refused for drafts and unless the bump kind is
/// `Patch` or `Minor` (also `Unknown` when the rule says `allow_unknown`, never `Major`) and CI
/// is strictly green (no checks is not enough). `Rebase` is refused
/// for drafts (it never merges). Merged and closed PRs only allow `MarkRead`, `MarkDone` and
/// `Unsubscribe`. Every other action is allowed.
pub fn auto_allowed(rule: &Rule, item: &PrItem, action: &Action) -> bool {
    if item.via_scope_only() && rule.involved != Involvement::ScopeOnly {
        return false;
    }
    if item.state != PrState::Open {
        return action.is_notification_only();
    }
    match action {
        Action::Rebase => !item.is_draft(),
        Action::Merge(_) | Action::ApproveAndMerge(_) | Action::EnableAutoMerge(_) => {
            let kind_allowed = match bump_kind(item) {
                UpdateKind::Patch | UpdateKind::Minor => true,
                UpdateKind::Unknown => rule.allow_unknown,
                UpdateKind::Major => false,
            };
            !item.is_draft() && kind_allowed && item.ci() == CiSummary::Green
        }
        _ => true,
    }
}

/// Problems that make rules useless or ambiguous: duplicate names, invalid title regexes,
/// repository globs that never match and rules without actions. Empty when everything is fine.
pub fn validate(rules: &[Rule]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut names = HashSet::new();
    for rule in rules {
        let name = &rule.name;
        if !names.insert(name.as_str()) {
            problems.push(format!(
                "rule `{name}`: the name is used more than once; give every rule its own name"
            ));
        }
        if let Err(error) = compile_title(rule) {
            problems.push(format!("rule `{name}`: invalid title regex: {error}"));
        }
        for glob in &rule.repos {
            if let Some(problem) = glob_problem(glob) {
                problems.push(format!("rule `{name}`: repos `{glob}` {problem}"));
            }
        }
        if rule.actions.is_empty() {
            problems.push(format!("rule `{name}`: no actions"));
        }
    }
    problems
}

/// Parses the contents of `rules.toml`. Unknown keys and actions are errors.
pub fn load_rules(toml_str: &str) -> Result<RuleSet, String> {
    toml::from_str(toml_str).map_err(|error| error.to_string())
}

/// A commented `rules.toml` without any active rule.
pub fn default_rules_toml() -> &'static str {
    DEFAULT_RULES
}

const DEFAULT_RULES: &str = r#"# Prism automation rules. Every [[rule]] needs a name and actions. Filters you leave out
# match everything, except checks (default: green) and pr_state (default: open only).
# Unknown keys are errors. Draft pull requests never match.
#
#   authors  logins (case-insensitive), e.g. ["renovate[bot]", "dependabot[bot]"]
#   repos    globs over owner/name: "acme/*", "*/widget", "acme/**"
#   title    regular expression over the pull request title
#   update   patch | minor | major | unknown (parsed from "from X to Y" / "to vY";
#            titles that are no version bump count as unknown)
#   checks   green | any (green: all checks pass, or no checks and the PR is clean)
#   state    clean | blocked | unstable | dirty | behind | draft | has-hooks | unknown
#            (GitHub's merge state; empty = any)
#   pr_state open | merged | closed (empty = open only; merged and closed PRs show up
#            while they still have a notification)
#   duplicate  true = only superseded duplicates, false = only non-duplicates
#   involved   true = only PRs that involve you (notification, review request, your own);
#              false = only PRs that are in the inbox through the [scope] of settings.toml
#   actions  approve, merge[:squash|merge|rebase], approve-and-merge, auto-merge, close[:"text"],
#            comment:"text", mark-read, mark-done, unsubscribe, rerun-failed,
#            rebase
#   auto     false = only suggest in "Planned actions"; true = run on every refresh
#            (never merges major/unknown updates or PRs without green checks, and only acts
#            on PRs that involve you unless the rule says involved = false; merged and closed
#            PRs only ever get mark-read, mark-done and unsubscribe)
#   allow_unknown  true = an auto rule may also merge updates that are no recognized version
#            bump (update = unknown, e.g. "docs: update OpenWiki"); major is still refused
#            (default: false)

# No rules are active by default. Copy examples from
# https://github.com/dfuchss/prism/tree/main/examples/rules, for instance:
#
# [[rule]]
# name = "approve green bot PRs"
# authors = ["dependabot[bot]", "renovate[bot]"]
# checks = "green"
# actions = ["approve"]
# auto = false
"#;

#[cfg(test)]
mod tests;
