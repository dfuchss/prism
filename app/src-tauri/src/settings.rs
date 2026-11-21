//! `settings.toml`: which repositories add all their open pull requests to the inbox (or are
//! hidden from it), and the `gh` executable. Unknown keys are errors.

use prism_rules::{glob_match, glob_problem};
use serde::{Deserialize, Serialize};

/// The whole `settings.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    #[serde(default)]
    pub(crate) scope: Scope,
    #[serde(default)]
    pub(crate) gh: Gh,
}

/// The GitHub CLI used to fetch the token.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Gh {
    /// Path of the `gh` executable; empty means `gh` from `PATH`.
    #[serde(default)]
    pub(crate) path: String,
}

/// Repositories whose open pull requests are shown even without involving the user. Both lists
/// hold globs over `owner/name` (see [`glob_match`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scope {
    /// Empty: no repository is added.
    #[serde(default)]
    pub(crate) include: Vec<String>,
    /// Wins over `include`.
    #[serde(default)]
    pub(crate) exclude: Vec<String>,
}

impl Scope {
    /// Whether the scope can add anything at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.include.is_empty()
    }

    /// Whether `repo` matches an include glob and no exclude glob.
    pub(crate) fn covers(&self, repo: &str) -> bool {
        let any = |globs: &[String]| globs.iter().any(|glob| glob_match(glob, repo));
        any(&self.include) && !any(&self.exclude)
    }

    /// Whether `repo` matches an exclude glob: it is hidden from the whole inbox, whatever
    /// brought it in (notification, review request, authorship or the scope).
    pub(crate) fn excludes(&self, repo: &str) -> bool {
        self.exclude.iter().any(|glob| glob_match(glob, repo))
    }

    /// The repositories of `repos` an exclude glob matches, in the given order.
    pub(crate) fn excluded_repos(&self, repos: &[String]) -> Vec<String> {
        repos
            .iter()
            .filter(|repo| self.excludes(repo))
            .cloned()
            .collect()
    }

    /// The repositories of `repos` the scope covers, in the given order.
    pub(crate) fn filter_repos(&self, repos: &[String]) -> Vec<String> {
        repos
            .iter()
            .filter(|repo| self.covers(repo))
            .cloned()
            .collect()
    }
}

/// Parses the contents of `settings.toml`. Unknown keys are errors.
pub(crate) fn load_settings(text: &str) -> Result<Settings, String> {
    toml::from_str(text).map_err(|error| error.to_string())
}

/// Patterns that can never match (empty, or without the `owner/name` slash); empty when fine.
pub(crate) fn validate_settings(settings: &Settings) -> Vec<String> {
    let scope = &settings.scope;
    let lists = [("include", &scope.include), ("exclude", &scope.exclude)];
    let mut problems = Vec::new();
    for (name, globs) in lists {
        for glob in globs {
            if let Some(problem) = glob_problem(glob) {
                problems.push(format!("{name} `{glob}` {problem}"));
            }
        }
    }
    problems
}

/// A commented `settings.toml` that changes nothing (empty `include`).
pub(crate) fn default_settings_toml() -> &'static str {
    DEFAULT_SETTINGS
}

const DEFAULT_SETTINGS: &str = r#"# Prism settings.

[scope]
# Open PRs of every repo you can access that matches one of these globs are added to the inbox.
# Globs are over owner/name: "owner/*", "*/prism", "owner/repo". Empty: only PRs that involve you.
include = []          # e.g. ["example-org/*"]
# Optional. Repos matching these globs are hidden from the whole inbox: notifications, review
# requests and your own PRs too (rules never see them). Wins over include.
exclude = []

[gh]
# Path of the gh executable; empty = "gh" from PATH. GH_TOKEN and GITHUB_TOKEN take precedence.
path = ""
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<String> {
        ["acme/widget", "acme/gadget", "Other/Thing", "solo/x"]
            .map(str::to_owned)
            .to_vec()
    }

    fn scope(include: &[&str], exclude: &[&str]) -> Scope {
        Scope {
            include: include.iter().map(|g| (*g).to_owned()).collect(),
            exclude: exclude.iter().map(|g| (*g).to_owned()).collect(),
        }
    }

    #[test]
    fn default_is_empty_and_parses() {
        let settings = load_settings(default_settings_toml()).unwrap();
        assert_eq!(settings, Settings::default());
        assert!(settings.scope.is_empty());
        assert!(settings.scope.filter_repos(&repos()).is_empty());
        assert!(validate_settings(&settings).is_empty());
    }

    #[test]
    fn include_and_exclude_globs_filter_repos() {
        let scope = scope(&["acme/*", "other/thing"], &["acme/gadget"]);
        assert_eq!(scope.filter_repos(&repos()), ["acme/widget", "Other/Thing"]);
    }

    #[test]
    fn exclude_matches_independently_of_include() {
        let scope = scope(&[], &["acme/gadget", "solo/*"]);
        assert!(scope.excludes("Acme/Gadget") && scope.excludes("solo/x"));
        assert!(!scope.excludes("acme/widget"));
        assert_eq!(scope.excluded_repos(&repos()), ["acme/gadget", "solo/x"]);
    }

    #[test]
    fn missing_sections_and_keys_get_defaults() {
        assert_eq!(load_settings("").unwrap(), Settings::default());
        let settings = load_settings("[scope]\ninclude = [\"a/*\"]\n").unwrap();
        assert_eq!(settings.scope, scope(&["a/*"], &[]));
    }

    #[test]
    fn bad_toml_types_and_unknown_keys_are_errors() {
        for text in [
            "[scope\n",
            "[scope]\ninclude = \"a/*\"\n",
            "[scope]\ninclde = [\"a/*\"]\n",
            "[gh]\npth = \"/opt/gh\"\n",
            "[scopes]\n",
        ] {
            assert!(load_settings(text).is_err(), "{text}");
        }
    }

    #[test]
    fn example_file_parses_and_validates() {
        let text = include_str!("../../../examples/rules/settings.example.toml");
        let settings = load_settings(text).unwrap();
        assert!(validate_settings(&settings).is_empty());
    }

    #[test]
    fn gh_path_defaults_to_empty_and_parses() {
        assert_eq!(load_settings("").unwrap().gh.path, "");
        let settings = load_settings("[gh]\npath = \"/opt/gh\"\n").unwrap();
        assert_eq!(settings.gh.path, "/opt/gh");
    }

    #[test]
    fn useless_patterns_are_reported() {
        let settings = Settings {
            scope: scope(&["acme", "a/*", ""], &["**"]),
            gh: Gh::default(),
        };
        assert_eq!(validate_settings(&settings).len(), 2);
    }
}
