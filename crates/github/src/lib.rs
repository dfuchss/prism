//! The GitHub client of Prism: authentication, fetching the inbox and running actions.
//!
//! - `client`: the transport (headers, one retry after a 401, rate limits, error mapping,
//!   REST and GraphQL); `token`: where the token comes from; `gql`: GraphQL helpers.
//! - Fetching, each module with its pure parser: `notifications`, `search`, `details`,
//!   `scope`; `inbox` joins them. `mapping` turns GitHub's enum strings into protocol types;
//!   `bump` reads dependency bumps from titles and bodies, `duplicates` flags superseded ones.
//! - Acting: `actions` (`Client::perform`) dispatches to `mutations` (head-pinned GraphQL),
//!   `notifications` (thread REST calls), `rebase` and `workflows` (failed jobs, re-runs).

mod actions;
mod bump;
mod client;
mod details;
mod duplicates;
mod error;
mod gql;
#[cfg(test)]
mod http_tests;
mod inbox;
mod mapping;
mod mutations;
mod notifications;
mod rebase;
mod scope;
mod search;
mod token;
mod workflows;

pub use client::Client;
pub use details::Details;
pub use duplicates::mark_duplicates;
pub use error::{Error, Result};

/// Locks a mutex, ignoring poisoning (every guarded value is replaced as a whole).
fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
