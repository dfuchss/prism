//! Where the API token comes from. The token lives in memory only and is never logged.

use crate::error::{Error, Result};
use crate::lock;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The environment variables checked before `gh`, in order.
const TOKEN_VARS: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

type Provider = Arc<dyn Fn() -> Result<String> + Send + Sync>;

/// Resolves the token lazily and caches it until [`TokenSource::invalidate`].
pub(crate) struct TokenSource {
    provider: Provider,
    cached: Mutex<Option<String>>,
    /// Grows with every invalidation, so a token resolved meanwhile is not cached.
    generation: AtomicU64,
    /// Lets one request resolve the token while the others wait (without blocking a thread).
    resolving: tokio::sync::Mutex<()>,
    /// The `gh` executable; empty means `gh` from `PATH`.
    gh_path: Arc<Mutex<String>>,
}

impl TokenSource {
    /// `GH_TOKEN`, then `GITHUB_TOKEN`, then `gh auth token` (with the path of
    /// [`TokenSource::set_gh_path`], `gh` from `PATH` at first).
    pub(crate) fn new() -> Self {
        let gh_path = Arc::new(Mutex::new(String::new()));
        let shared = Arc::clone(&gh_path);
        let provider: Provider = Arc::new(move || {
            let path = lock(&shared).clone();
            resolve_token(&path)
        });
        Self::build(provider, gh_path)
    }

    /// A source with a custom provider.
    #[cfg(test)]
    pub(crate) fn with_provider(
        provider: impl Fn() -> Result<String> + Send + Sync + 'static,
    ) -> Self {
        Self::build(Arc::new(provider), Arc::default())
    }

    fn build(provider: Provider, gh_path: Arc<Mutex<String>>) -> Self {
        Self {
            provider,
            cached: Mutex::new(None),
            generation: AtomicU64::new(0),
            resolving: tokio::sync::Mutex::new(()),
            gh_path,
        }
    }

    /// Sets the `gh` executable (empty: `gh` from `PATH`) and forgets the cached token, so the
    /// change takes effect on the next request.
    pub(crate) fn set_gh_path(&self, path: &str) {
        path.trim().clone_into(&mut lock(&self.gh_path));
        self.invalidate();
    }

    /// Names where the token comes from (never the token): `GH_TOKEN`, `GITHUB_TOKEN` or
    /// `gh at <path>`.
    pub(crate) fn describe(&self) -> String {
        if let Some((var, _)) = env_token() {
            return var.to_owned();
        }
        format!("gh at {}", locate_gh(&lock(&self.gh_path)))
    }

    /// The cached token, or a new one. `gh auth token` runs a process, so it runs on the
    /// blocking pool, never on an async worker.
    pub(crate) async fn token(&self) -> Result<String> {
        if let Some(token) = lock(&self.cached).clone() {
            return Ok(token);
        }
        let _resolving = self.resolving.lock().await;
        if let Some(token) = lock(&self.cached).clone() {
            return Ok(token);
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let provider = Arc::clone(&self.provider);
        let token = tokio::task::spawn_blocking(move || provider())
            .await
            .map_err(|_| Error::TokenUnavailable)??;
        let mut cached = lock(&self.cached);
        if self.generation.load(Ordering::SeqCst) == generation {
            *cached = Some(token.clone());
        }
        Ok(token)
    }

    /// Forgets the cached token so the next call fetches a new one.
    pub(crate) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        *lock(&self.cached) = None;
    }
}

/// The first non-empty token variable and its value.
fn env_token() -> Option<(&'static str, String)> {
    TOKEN_VARS.into_iter().find_map(|var| {
        let value = std::env::var(var).ok()?;
        non_empty(&value).map(|token| (var, token))
    })
}

fn resolve_token(gh_path: &str) -> Result<String> {
    match env_token() {
        Some((_, token)) => Ok(token),
        None => gh_token(gh_path),
    }
}

/// Runs `<gh> auth token`; an empty `gh_path` means `gh` from `PATH`.
fn gh_token(gh_path: &str) -> Result<String> {
    let program = non_empty(gh_path).unwrap_or_else(|| "gh".to_owned());
    let mut command = Command::new(&program);
    command.args(["auth", "token"]);
    #[cfg(windows)]
    {
        // No console window flashing up behind the GUI app.
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().map_err(|_| Error::GhNotStarted(program))?;
    if !output.status.success() {
        return Err(Error::TokenUnavailable);
    }
    non_empty(&String::from_utf8_lossy(&output.stdout)).ok_or(Error::TokenUnavailable)
}

/// The configured `gh` path, or the first `gh` on `PATH` (plain `gh` when none is found).
fn locate_gh(configured: &str) -> String {
    if let Some(path) = non_empty(configured) {
        return path;
    }
    let executable = if cfg!(windows) { "gh.exe" } else { "gh" };
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(executable))
                .find(|candidate| candidate.is_file())
        })
        .map_or_else(|| "gh".to_owned(), |path| path.display().to_string())
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn counting() -> (TokenSource, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let source = TokenSource::with_provider(move || {
            Ok(format!("t{}", counter.fetch_add(1, Ordering::SeqCst)))
        });
        (source, calls)
    }

    #[tokio::test]
    async fn caches_until_invalidated() {
        let (source, _) = counting();
        assert_eq!(source.token().await.unwrap(), "t0");
        assert_eq!(source.token().await.unwrap(), "t0");
        source.invalidate();
        assert_eq!(source.token().await.unwrap(), "t1");
    }

    #[tokio::test]
    async fn concurrent_requests_resolve_once() {
        let (source, calls) = counting();
        let (a, b) = tokio::join!(source.token(), source.token());
        assert_eq!((a.unwrap(), b.unwrap()), ("t0".to_owned(), "t0".to_owned()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn missing_gh_names_the_path_it_tried() {
        let error = gh_token("/nonexistent/dir/gh").unwrap_err();
        let text = error.to_string();
        assert!(text.contains("/nonexistent/dir/gh"), "{text}");
        assert!(
            text.contains("[gh] path") && text.contains("gh auth login"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn uses_the_configured_gh() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("prism-test-gh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("gh");
        std::fs::write(&script, "#!/bin/sh\necho tok-from-script\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = script.to_str().unwrap();
        assert_eq!(gh_token(path).unwrap(), "tok-from-script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(gh_token(path), Err(Error::GhNotStarted(_))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn set_gh_path_forgets_the_cached_token() {
        let (source, _) = counting();
        assert_eq!(source.token().await.unwrap(), "t0");
        source.set_gh_path(" /x/gh ");
        assert!(lock(&source.cached).is_none());
        assert_eq!(lock(&source.gh_path).as_str(), "/x/gh");
        assert_eq!(locate_gh(" /x/gh "), "/x/gh");
    }

    #[test]
    fn non_empty_trims() {
        assert_eq!(non_empty(" abc\n").as_deref(), Some("abc"));
        assert_eq!(non_empty("  \n"), None);
    }
}
