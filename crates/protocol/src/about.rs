//! Facts about the running app, shown on the About tab.

use serde::{Deserialize, Serialize};

/// What the scope of an (unsaved) `settings.toml` means for the accessible repositories.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopePreview {
    /// Repositories added to the inbox by `include`.
    pub covered: Vec<String>,
    /// Repositories hidden from the whole inbox by `exclude`.
    pub excluded: Vec<String>,
}

/// Version, platform and GitHub connection status. Never contains the token itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub version: String,
    pub tauri_version: String,
    /// Operating system and architecture, e.g. `macos aarch64`.
    pub platform: String,
    pub config_dir: String,
    /// Where the token comes from: `GH_TOKEN`, `GITHUB_TOKEN` or `gh at <path>`.
    pub auth_source: String,
    pub viewer: Viewer,
}

/// The GitHub connection as seen by the About tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Viewer {
    /// The login of the signed-in user.
    SignedIn(String),
    /// Why GitHub could not be reached or rejected the token.
    Unavailable(String),
}
