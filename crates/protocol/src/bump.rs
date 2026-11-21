//! Dependency bumps as announced by pull request titles (the parsing lives in the GitHub crate).

use serde::{Deserialize, Serialize};

/// Kind of a dependency bump. The names are also the `update` values of `rules.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateKind {
    Patch,
    Minor,
    Major,
    Unknown,
}

/// A dependency update as announced by a pull request title (and completed from its body).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bump {
    pub dependency: String,
    /// `None` for Renovate titles whose body names no version either.
    pub from: Option<String>,
    /// Target version without a leading `v`.
    pub to: String,
    pub kind: UpdateKind,
    /// The directory of a Dependabot bump (`... in /subdir`); `None` for the root and Renovate.
    pub directory: Option<String>,
}
