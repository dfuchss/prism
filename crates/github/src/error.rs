//! Error type of the GitHub client. The messages end up in banners, toasts and the audit log.

/// Everything that can go wrong while talking to GitHub.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No token in the environment and `gh` could not provide one.
    #[error("gh is not installed or not logged in: run `gh auth login`")]
    TokenUnavailable,
    /// `gh` could not be started at all (not found, not executable).
    #[error(
        "could not start `{0}`: put gh on PATH, set `[gh] path` in the Settings tab or run `gh auth login`"
    )]
    GhNotStarted(String),
    /// GitHub rejected the token, even after fetching a fresh one.
    #[error("GitHub rejected the token (HTTP 401); run `gh auth login` or fix GH_TOKEN")]
    Unauthorized,
    /// GitHub's rate limit is exhausted; the poller waits at least `retry_after_secs`.
    #[error("GitHub rate limit reached; trying again in {retry_after_secs} s")]
    RateLimited { retry_after_secs: u64 },
    /// Any other non-success status, with GitHub's `message` (or a short excerpt of the body).
    #[error("GitHub returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("network error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("GraphQL error: {0}")]
    GraphQl(String),
    /// GitHub answered with something Prism cannot read.
    #[error("unexpected response from GitHub: {0}")]
    Parse(String),
    /// Prism refuses the action before asking GitHub (no thread, method not allowed, ...).
    #[error("{0}")]
    Refused(String),
}

pub type Result<T> = std::result::Result<T, Error>;
