//! The schema of `rules.toml`. Unknown keys are errors: a misspelled filter must not silently
//! widen a rule.

use protocol::{Action, MergeState, PrState, UpdateKind};
use serde::{Deserialize, Serialize};

/// Which CI state a rule requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChecksReq {
    #[default]
    Green,
    Any,
}

/// The `duplicate` filter: `true` matches only flagged duplicates, `false` only
/// non-duplicates, leaving it out matches both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "Option<bool>", into = "Option<bool>")]
pub enum DuplicateFilter {
    #[default]
    Any,
    OnlyDuplicates,
    NoDuplicates,
}

impl DuplicateFilter {
    pub fn accepts(self, is_duplicate: bool) -> bool {
        match self {
            Self::Any => true,
            Self::OnlyDuplicates => is_duplicate,
            Self::NoDuplicates => !is_duplicate,
        }
    }
}

impl From<Option<bool>> for DuplicateFilter {
    fn from(value: Option<bool>) -> Self {
        match value {
            None => Self::Any,
            Some(true) => Self::OnlyDuplicates,
            Some(false) => Self::NoDuplicates,
        }
    }
}

impl From<DuplicateFilter> for Option<bool> {
    fn from(filter: DuplicateFilter) -> Self {
        match filter {
            DuplicateFilter::Any => None,
            DuplicateFilter::OnlyDuplicates => Some(true),
            DuplicateFilter::NoDuplicates => Some(false),
        }
    }
}

/// The `involved` filter: `true` matches only PRs that involve the viewer (a notification, a
/// review request or their own PR), `false` only PRs in the inbox through the `settings.toml`
/// scope alone, leaving it out matches both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "Option<bool>", into = "Option<bool>")]
pub enum Involvement {
    #[default]
    Any,
    Involved,
    ScopeOnly,
}

impl Involvement {
    pub fn accepts(self, via_scope_only: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Involved => !via_scope_only,
            Self::ScopeOnly => via_scope_only,
        }
    }
}

impl From<Option<bool>> for Involvement {
    fn from(value: Option<bool>) -> Self {
        match value {
            None => Self::Any,
            Some(true) => Self::Involved,
            Some(false) => Self::ScopeOnly,
        }
    }
}

impl From<Involvement> for Option<bool> {
    fn from(filter: Involvement) -> Self {
        match filter {
            Involvement::Any => None,
            Involvement::Involved => Some(true),
            Involvement::ScopeOnly => Some(false),
        }
    }
}

/// One automation rule (a `[[rule]]` table). Filters left out match everything, except
/// `checks` (default `green`) and `pr_state` (default open only).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    /// Logins, compared case-insensitively.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Glob patterns over `owner/name`.
    #[serde(default)]
    pub repos: Vec<String>,
    /// Regular expression over the title.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, rename = "update")]
    pub update_kinds: Vec<UpdateKind>,
    #[serde(default)]
    pub checks: ChecksReq,
    /// Merge states the PR must be in (GitHub's `mergeStateStatus`, TOML key `state`); empty
    /// matches any.
    #[serde(default, rename = "state")]
    pub merge_state: Vec<MergeState>,
    /// Pull request states the rule applies to; empty means `["open"]`, so rules never act on
    /// merged or closed PRs unless they say so.
    #[serde(default)]
    pub pr_state: Vec<PrState>,
    #[serde(default)]
    pub duplicate: DuplicateFilter,
    #[serde(default)]
    pub involved: Involvement,
    #[serde(with = "action_strings", default)]
    pub actions: Vec<Action>,
    /// `false`: suggestions only; `true`: runs on every refresh.
    #[serde(default)]
    pub auto: bool,
}

/// The whole `rules.toml`: a list of `[[rule]]` tables.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSet {
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

/// Serde adapter for the actions of a rule, which `rules.toml` writes as strings
/// (`"merge:squash"`, see `Action`'s `Display`).
mod action_strings {
    use protocol::Action;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        actions: &[Action],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        actions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Action>, D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .iter()
            .map(|text| text.parse().map_err(D::Error::custom))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::MergeMethod;

    fn parse(text: &str) -> Result<RuleSet, toml::de::Error> {
        toml::from_str(text)
    }

    const EXAMPLE: &str = r#"
[[rule]]
name = "green patch/minor bot bumps"
authors = ["renovate[bot]", "dependabot[bot]"]
repos = ["acme/*"]
update = ["patch", "minor"]
checks = "green"
state = ["behind", "has-hooks"]
actions = ["approve", "merge"]
auto = false

[[rule]]
name = "close duplicates"
duplicate = true
involved = false
title = "^(chore|build)\\(deps\\)"
checks = "any"
pr_state = ["open", "merged", "closed"]
actions = ["comment:\"Superseded.\"", "close", "mark-done"]
"#;

    #[test]
    fn every_key_parses() {
        let set = parse(EXAMPLE).unwrap();
        let first = &set.rules[0];
        assert_eq!(first.authors.len(), 2);
        assert_eq!(first.update_kinds, [UpdateKind::Patch, UpdateKind::Minor]);
        assert_eq!(first.checks, ChecksReq::Green);
        assert_eq!(
            first.merge_state,
            [MergeState::Behind, MergeState::HasHooks]
        );
        assert_eq!(first.actions, [Action::Approve, Action::Merge(None)]);
        assert!(!first.auto);
        let second = &set.rules[1];
        assert_eq!(second.duplicate, DuplicateFilter::OnlyDuplicates);
        assert_eq!(second.involved, Involvement::ScopeOnly);
        assert!(second.title.is_some());
        assert_eq!(second.checks, ChecksReq::Any);
        assert_eq!(second.pr_state.len(), 3);
        assert_eq!(second.actions[0], Action::Comment("Superseded.".to_owned()));
    }

    #[test]
    fn minimal_rule_gets_defaults() {
        let set = parse("[[rule]]\nname = \"x\"\n").unwrap();
        let rule = &set.rules[0];
        assert!(rule.actions.is_empty() && !rule.auto && rule.pr_state.is_empty());
        assert_eq!(rule.checks, ChecksReq::Green);
        assert_eq!(rule.duplicate, DuplicateFilter::Any);
        assert_eq!(rule.involved, Involvement::Any);
        assert!(
            parse("[[rule]]\nauto = true\n").is_err(),
            "the name is required"
        );
    }

    #[test]
    fn unknown_keys_and_values_are_errors() {
        for text in [
            "[[rule]]\nname = \"x\"\nauthor = [\"renovate[bot]\"]\n",
            "[[rule]]\nname = \"x\"\nmerge_state = [\"clean\"]\n",
            "[[rules]]\nname = \"x\"\n",
            "rule_count = 1\n",
            "[[rule]]\nname = \"x\"\nstate = [\"stale\"]\n",
            "[[rule]]\nname = \"x\"\npr_state = [\"gone\"]\n",
            "[[rule]]\nname = \"x\"\nduplicate = \"yes\"\n",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        let error = parse("[[rule]]\nname = \"x\"\nauthor = []\n").unwrap_err();
        assert!(error.to_string().contains("author"), "{error}");
    }

    #[test]
    fn toml_roundtrip() {
        let set = RuleSet {
            rules: vec![
                Rule {
                    name: "r".to_owned(),
                    authors: vec!["a".to_owned()],
                    update_kinds: vec![UpdateKind::Major],
                    checks: ChecksReq::Any,
                    merge_state: vec![MergeState::Behind, MergeState::HasHooks],
                    pr_state: vec![PrState::Merged, PrState::Closed],
                    duplicate: DuplicateFilter::NoDuplicates,
                    involved: Involvement::Involved,
                    actions: vec![Action::Merge(Some(MergeMethod::Squash)), Action::MarkDone],
                    auto: true,
                    ..Rule::default()
                },
                Rule {
                    name: "defaults".to_owned(),
                    ..Rule::default()
                },
            ],
        };
        let text = toml::to_string(&set).unwrap();
        assert!(
            text.contains("state = [\"behind\", \"has-hooks\"]"),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap(), set);
    }
}
