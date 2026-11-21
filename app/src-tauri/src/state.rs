//! Shared application state.
//!
//! # Locking
//!
//! Three async locks order the background work, always taken in this order and released in
//! reverse: `refresh_lock` (one refresh at a time) → `auto_gate` (one auto pass at a time; a
//! [`PassGate`] that never waits, a pass requested meanwhile runs afterwards) → `action_lock`
//! (one batch of actions at a time, taken inside `exec::execute`). A manual batch only takes
//! `action_lock`; the auto pass and refresh after it run once the batch released it.
//!
//! The `std` mutexes guard plain data. Their guards are never held across an `.await` and never
//! two at a time, so they cannot deadlock with each other or with the async locks. The atomics
//! need no lock.

use crate::config;
use crate::exec::AutoMemo;
use crate::overrides::Overrides;
use crate::refresh::PassGate;
use crate::scope::RepoCache;
use crate::settings::{Settings, load_settings};
use crate::source::Source;
use prism_github::Client;
use prism_rules::RuleSet;
use protocol::InboxSnapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Mutex, MutexGuard};

/// A configuration file's content, or why it could not be loaded (the default content is used
/// then: no rules, an empty scope).
pub(crate) struct Loaded<T> {
    pub value: T,
    pub error: Option<String>,
}

impl<T> Loaded<T> {
    /// Content that loaded (or was saved) without problems.
    pub(crate) fn ok(value: T) -> Self {
        Self { value, error: None }
    }
}

impl<T: Default> Loaded<T> {
    /// The parsed text, or the default with `prefix: error` as the error.
    fn parse<E: std::fmt::Display>(parsed: Result<T, E>, prefix: &str) -> Self {
        match parsed {
            Ok(value) => Self::ok(value),
            Err(error) => Self {
                value: T::default(),
                error: Some(format!("{prefix}: {error}")),
            },
        }
    }
}

pub(crate) struct AppState {
    /// GitHub, or the demo data (`PRISM_DEMO=1`).
    pub client: Source,
    pub snapshot: Mutex<Option<InboxSnapshot>>,
    pub rules: Mutex<Loaded<RuleSet>>,
    pub settings: Mutex<Loaded<Settings>>,
    /// Counts settings changes. A fetch started before a change does not store its snapshot
    /// (it was made for the old scope).
    pub settings_generation: AtomicU64,
    pub repo_cache: Mutex<RepoCache>,
    pub config_dir: PathBuf,
    /// Serializes refreshes (poller, button, after actions).
    pub refresh_lock: tokio::sync::Mutex<()>,
    /// Serializes action runs (manual, planned, auto), so each one re-fetches fresh details
    /// after the previous one finished and never acts on what another run just changed.
    pub action_lock: tokio::sync::Mutex<()>,
    /// Serializes auto passes; a pass requested meanwhile runs afterwards.
    pub auto_gate: PassGate,
    /// Thread and PR state changes made here that GitHub may not report yet.
    pub overrides: Mutex<Overrides>,
    /// Numbers the action batches (for `action-progress` events).
    pub batch_counter: AtomicU64,
    pub memo: Mutex<AutoMemo>,
    /// Background refreshes running (for the `refreshing` event).
    pub background_refreshes: AtomicUsize,
}

impl AppState {
    /// Creates the config directory, `rules.toml` and `settings.toml` (with the defaults) when
    /// missing. A `rules.toml` that does not parse yields an empty rule set and an error, a
    /// `settings.toml` that does not parse an empty scope and an error; the files stay
    /// untouched. Talks to GitHub.
    pub(crate) fn load(config_dir: PathBuf) -> std::io::Result<Self> {
        Self::with_source(config_dir, Source::GitHub(Client::new()))
    }

    /// Like [`AppState::load`], with the given source of the inbox.
    pub(crate) fn with_source(config_dir: PathBuf, client: Source) -> std::io::Result<Self> {
        let text = config::ensure_rules(&config_dir)?;
        let settings_text = config::ensure_settings(&config_dir)?;
        let rules = Loaded::parse(
            prism_rules::load_rules(&text),
            "rules.toml is invalid, no rules are active",
        );
        let settings = Loaded::parse(
            load_settings(&settings_text),
            "settings.toml is invalid, the repository scope is empty",
        );
        let memo = AutoMemo::new(config::read_memo(&config_dir));
        client.set_gh_path(&settings.value.gh.path);
        Ok(Self {
            client,
            snapshot: Mutex::new(None),
            rules: Mutex::new(rules),
            settings: Mutex::new(settings),
            settings_generation: AtomicU64::new(0),
            repo_cache: Mutex::new(RepoCache::default()),
            config_dir,
            refresh_lock: tokio::sync::Mutex::new(()),
            action_lock: tokio::sync::Mutex::new(()),
            auto_gate: PassGate::default(),
            overrides: Mutex::new(Overrides::default()),
            batch_counter: AtomicU64::new(0),
            memo: Mutex::new(memo),
            background_refreshes: AtomicUsize::new(0),
        })
    }
}

/// Locks a mutex, ignoring poisoning: every guarded value stays usable after a panic halfway
/// through an update (at worst an override or memo entry is missing, which a refresh or the
/// fresh re-check before every action makes up for).
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_rules_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("rules.toml"), "[[rule]\nname = ").unwrap();
        let state = AppState::load(dir.path().to_owned()).unwrap();
        let rules = lock(&state.rules);
        assert!(rules.value.rules.is_empty());
        let error = rules.error.clone().unwrap();
        assert!(error.contains("rules.toml is invalid"), "{error}");
    }

    #[test]
    fn invalid_settings_are_reported_and_leave_the_scope_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.toml"), "[scope\ninclude = ").unwrap();
        let state = AppState::load(dir.path().to_owned()).unwrap();
        let settings = lock(&state.settings);
        assert!(settings.value.scope.is_empty());
        let error = settings.error.clone().unwrap();
        assert!(error.contains("settings.toml is invalid"), "{error}");
    }
}
