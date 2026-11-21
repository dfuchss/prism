//! Read-only smoke test against real GitHub: `cargo run -p prism-github --example smoke`.
//! Prints bucket counts, duplicates and update kinds of the inbox. Mutates nothing.
//!
//! With `PRISM_SCOPE_INCLUDE="owner/*,other/repo"` the open PRs of the accessible repos matching
//! those globs are added, like the `[scope]` of `settings.toml`.

#![allow(clippy::print_stdout)]

use prism_github::{Client, mark_duplicates};
use prism_rules::glob_match;
use protocol::{Action, PrItem, PrRef, PrState, bucket, unavailable_reason};
use std::collections::BTreeMap;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = Client::new();
    let scope = scope_refs(&client).await;
    let mut snapshot = match client.inbox(&scope).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            println!("inbox failed: {error}");
            std::process::exit(1);
        }
    };
    mark_duplicates(&mut snapshot.items);
    println!(
        "{} items (fetched {}), warnings: {:?}",
        snapshot.items.len(),
        snapshot.fetched_at,
        snapshot.error
    );
    let closed = |state| snapshot.items.iter().filter(|i| i.state == state).count();
    println!(
        "merged with notification: {}, closed with notification: {}",
        closed(PrState::Merged),
        closed(PrState::Closed)
    );
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    for item in snapshot.items.iter().filter(|i| i.state == PrState::Open) {
        *buckets.entry(format!("{:?}", bucket(item))).or_default() += 1;
    }
    println!("buckets: {buckets:?}");
    let via_scope = snapshot.items.iter().filter(|i| i.via_scope_only()).count();
    println!(
        "scope PRs listed: {}, via scope only: {via_scope}",
        scope.len()
    );
    capability_summary(&snapshot.items);
    for item in snapshot.items.iter().filter(|i| i.state == PrState::Open) {
        let kind = item.bump.as_ref().map_or_else(
            || "-".to_owned(),
            |bump| format!("{:?} {:?}->{}", bump.kind, bump.from, bump.to),
        );
        let duplicate = item
            .duplicate_of
            .as_ref()
            .map_or(String::new(), |pr| format!("  DUP of #{}", pr.number));
        println!(
            "{:<10} {:<40} {:<18} {kind}{duplicate}",
            format!("{:?}", bucket(item)),
            item.pr,
            item.author
        );
    }
}

/// Counts the open PRs auto-merge or approving is unavailable for.
fn capability_summary(items: &[PrItem]) {
    let open = || items.iter().filter(|i| i.state == PrState::Open);
    let no_approve = open()
        .filter(|i| unavailable_reason(i, &Action::Approve).is_some())
        .count();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for item in open() {
        if let Some(reason) = unavailable_reason(item, &Action::EnableAutoMerge(None)) {
            *reasons.entry(reason).or_default() += 1;
        }
    }
    println!(
        "open: {}, auto-merge unavailable: {reasons:?}, own (cannot approve): {no_approve}",
        open().count()
    );
}

/// Open PRs of the repos matching `PRISM_SCOPE_INCLUDE` (comma-separated globs); none when unset.
/// Errors are printed and leave the scope empty.
async fn scope_refs(client: &Client) -> Vec<PrRef> {
    let include = std::env::var("PRISM_SCOPE_INCLUDE").unwrap_or_default();
    let globs: Vec<&str> = include.split(',').filter(|glob| !glob.is_empty()).collect();
    if globs.is_empty() {
        return Vec::new();
    }
    let repos = match client.accessible_repos().await {
        Ok(repos) => repos,
        Err(error) => {
            println!("could not list repositories: {error}");
            return Vec::new();
        }
    };
    let matched: Vec<String> = repos
        .iter()
        .filter(|repo| globs.iter().any(|glob| glob_match(glob, repo)))
        .cloned()
        .collect();
    println!(
        "{} accessible repos, {} in scope",
        repos.len(),
        matched.len()
    );
    client.open_pr_refs(&matched).await.unwrap_or_else(|error| {
        println!("could not list scope pull requests: {error}");
        Vec::new()
    })
}
