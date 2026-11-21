//! Parses dependency-bump pull request titles and bodies (Renovate and Dependabot) and
//! compares versions.

use protocol::{Bump, UpdateKind};
use std::cmp::Ordering;

/// Parses `Bump x from a to b` (Dependabot) and `Update [dependency|plugin] x to vB [(major)]`
/// (Renovate), with or without a conventional-commit prefix such as `chore(deps): `.
///
/// The kind is computed from `from`/`to` when both are known. Renovate titles have no `from`,
/// so there the kind is taken from a `(major)`/`(minor)`/`(patch)` suffix, else `Unknown`.
/// Returns `None` for anything else, including bumps without versions (`Bump react and
/// @types/react`) and grouped bumps.
pub(crate) fn parse_bump(title: &str) -> Option<Bump> {
    let title = strip_scope(title.trim());
    if let Some(rest) = strip_prefix_ci(title, "bump ") {
        parse_dependabot(rest)
    } else if let Some(rest) = strip_prefix_ci(title, "update ") {
        parse_renovate(rest)
    } else {
        None
    }
}

/// Finds the version a Renovate pull request bumps from in its body, whose update table has
/// rows like ``| [dep](https://...) | [`5.10.1` → `5.12.1`](https://...) | ...`` or
/// ``| `5.10.1` -> `5.12.1` |`` (arrow `→` or `->`). Takes the pair whose right side equals
/// `to` (ignoring a `v`), else the first pair.
fn from_version_in_body(body: &str, to: &str) -> Option<String> {
    let to = strip_v(to.trim());
    let mut first = None;
    for line in body.lines() {
        for (left, right) in arrow_pairs(line) {
            if strip_v(&right) == to {
                return Some(left);
            }
            first.get_or_insert(left);
        }
    }
    first
}

/// All `left -> right` version pairs of a line (backticks and brackets stripped).
fn arrow_pairs(line: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut rest = line;
    while let Some((start, len)) = find_arrow(rest) {
        let before = &rest[..start];
        let after = &rest[start + len..];
        if let (Some(left), Some(right)) = (version_before(before), version_after(after)) {
            pairs.push((left, right));
        }
        rest = after;
    }
    pairs
}

fn find_arrow(text: &str) -> Option<(usize, usize)> {
    let arrow = text.find('→').map(|at| (at, '→'.len_utf8()));
    let ascii = text.find("->").map(|at| (at, 2));
    match (arrow, ascii) {
        (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

fn is_version_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')
}

fn clean_version(raw: &str) -> Option<String> {
    let version = strip_v(raw);
    (version.starts_with(|c: char| c.is_ascii_digit())).then(|| version.to_owned())
}

fn version_before(text: &str) -> Option<String> {
    let trimmed =
        text.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '[' | ']'));
    let start = trimmed
        .rfind(|c: char| !is_version_char(c))
        .map_or(0, |at| {
            at + trimmed[at..].chars().next().map_or(1, char::len_utf8)
        });
    clean_version(&trimmed[start..])
}

fn version_after(text: &str) -> Option<String> {
    let trimmed =
        text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '[' | ']'));
    let end = trimmed
        .find(|c: char| !is_version_char(c))
        .unwrap_or(trimmed.len());
    clean_version(&trimmed[..end])
}

/// Builds the [`Bump`] of a pull request: the title's, with `from` completed from the body and
/// the kind recomputed when the title has no `from` version.
pub(crate) fn bump_of(title: &str, body: &str) -> Option<Bump> {
    let mut bump = parse_bump(title)?;
    if bump.from.is_none()
        && let Some(from) = from_version_in_body(body, &bump.to)
    {
        // The title's hint stays for versions that cannot be compared, but not for a
        // build-metadata-only change.
        bump.kind = match classify(&from, &bump.to) {
            UpdateKind::Unknown if !same_version(&from, &bump.to) => bump.kind,
            kind => kind,
        };
        bump.from = Some(from);
    }
    Some(bump)
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// Removes a leading `type(scope)!: ` (`Dependency(deps-dev): `, `chore(deps): `). A colon inside
/// a dependency name (`Bump junit:junit ...`) is not mistaken for one.
fn strip_scope(title: &str) -> &str {
    let Some((head, rest)) = title.split_once(':') else {
        return title;
    };
    let word_end = head
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(head.len());
    let tail = &head[word_end..];
    let scope_ok = tail.is_empty()
        || tail == "!"
        || (tail.starts_with('(') && (tail.ends_with(')') || tail.ends_with(")!")));
    if word_end > 0 && scope_ok {
        rest.trim_start()
    } else {
        title
    }
}

fn first_token(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
}

/// Drops a leading `v` in front of a digit (`v5.1` -> `5.1`).
fn strip_v(version: &str) -> &str {
    match version.strip_prefix(['v', 'V']) {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest,
        _ => version,
    }
}

fn parse_dependabot(rest: &str) -> Option<Bump> {
    let (dependency, versions) = rest.split_once(" from ")?;
    let (from, to) = versions.split_once(" to ")?;
    let directory = to
        .split_once(" in ")
        .map(|(_, directory)| first_token(directory))
        .filter(|directory| !directory.is_empty() && *directory != "/")
        .map(str::to_owned);
    let (dependency, from, to) = (
        dependency.trim(),
        strip_v(first_token(from)),
        strip_v(first_token(to)),
    );
    if dependency.is_empty() || from.is_empty() || to.is_empty() {
        return None;
    }
    Some(Bump {
        dependency: dependency.to_owned(),
        from: Some(from.to_owned()),
        to: to.to_owned(),
        kind: classify(from, to),
        directory,
    })
}

fn parse_renovate(rest: &str) -> Option<Bump> {
    let mut rest = rest.trim();
    let mut hint = UpdateKind::Unknown;
    if let Some(open) = rest.strip_suffix(')').and_then(|r| r.rfind(" (")) {
        hint = match rest[open + 2..rest.len() - 1].to_ascii_lowercase().as_str() {
            "major" => UpdateKind::Major,
            "minor" => UpdateKind::Minor,
            "patch" => UpdateKind::Patch,
            _ => UpdateKind::Unknown,
        };
        rest = rest[..open].trim_end();
    }
    for noise in ["dependency ", "plugin "] {
        rest = strip_prefix_ci(rest, noise).unwrap_or(rest);
    }
    let (dependency, version) = rest.rsplit_once(" to ")?;
    let (dependency, to) = (dependency.trim(), strip_v(first_token(version)));
    if dependency.is_empty() || to.is_empty() {
        return None;
    }
    Some(Bump {
        dependency: dependency.to_owned(),
        from: None,
        to: to.to_owned(),
        kind: hint,
        directory: None,
    })
}

fn segments(version: &str) -> Vec<&str> {
    strip_v(version)
        .split(['.', '-', '+'])
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// The version without semantic-versioning build metadata (`25.0.5+8.0.LTS` -> `25.0.5`).
fn without_build(version: &str) -> &str {
    let version = strip_v(version);
    version.split_once('+').map_or(version, |(base, _)| base)
}

/// Whether two versions are the same apart from build metadata and spelling (`1.2` and
/// `v1.2.0` are; `1.0.0-rc.1` and `1.0.0` are not).
fn same_version(a: &str, b: &str) -> bool {
    compare_versions(without_build(a), without_build(b)) == Ordering::Equal
}

/// The numeric `major.minor.patch` of a version (missing parts are 0, build metadata ignored);
/// `None` when any of the first three segments is not a number (`3-eclipse-temurin-25`).
fn core(version: &str) -> Option<[u64; 3]> {
    let mut parts = [0; 3];
    for (slot, segment) in parts.iter_mut().zip(segments(without_build(version))) {
        *slot = segment.parse().ok()?;
    }
    Some(parts)
}

/// Classifies a version change. Any difference in the major number is `Major`, in the minor
/// number `Minor` (also for `0.y.z`: Java libraries routinely break only on minors, so
/// `0.31 -> 0.33` is `Minor`), otherwise `Patch`. Non-numeric major/minor/patch segments (Docker
/// tags such as `3-eclipse-temurin-25`), identical versions and changes of the build metadata
/// only (`25.0.5+8.0.LTS -> 25.0.5+100000002.0.LTS`, which can hide any change) give `Unknown`.
fn classify(from: &str, to: &str) -> UpdateKind {
    let (Some(a), Some(b)) = (core(from), core(to)) else {
        return UpdateKind::Unknown;
    };
    if a[0] != b[0] {
        UpdateKind::Major
    } else if a[1] != b[1] {
        UpdateKind::Minor
    } else if a[2] != b[2] || !same_version(from, to) {
        UpdateKind::Patch
    } else {
        UpdateKind::Unknown
    }
}

/// Orders two version strings segment by segment (numbers numerically, missing segments count
/// as 0, non-numeric segments as text, a number sorts above text).
pub(crate) fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a, b) = (segments(a), segments(b));
    for index in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(index).unwrap_or(&"0"), b.get(index).unwrap_or(&"0"));
        let ordering = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Greater,
            (Err(_), Ok(_)) => Ordering::Less,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "| Package | Change | Age |\n|---|---|---|\n\
        | [left-pad](https://example.com/left-pad) | [`1.2.0` → `1.3.4`](https://example.com/c) | x |\n\
        | [other](https://example.com/o) | [`2.0.0` → `3.0.0`](https://example.com/d) | x |";

    #[test]
    fn body_picks_pair_matching_target() {
        assert_eq!(
            from_version_in_body(TABLE, "3.0.0").as_deref(),
            Some("2.0.0")
        );
        assert_eq!(
            from_version_in_body(TABLE, "v1.3.4").as_deref(),
            Some("1.2.0")
        );
        assert_eq!(
            from_version_in_body(TABLE, "9.9.9").as_deref(),
            Some("1.2.0")
        );
    }

    #[test]
    fn body_plain_ascii_arrow() {
        let body = "| left-pad | `v1.2.0` -> `v1.2.1` |";
        assert_eq!(
            from_version_in_body(body, "1.2.1").as_deref(),
            Some("1.2.0")
        );
        assert_eq!(from_version_in_body("no table here", "1"), None);
        assert_eq!(from_version_in_body("a -> b", "1"), None);
    }

    #[test]
    fn bump_of_completes_renovate_kind() {
        let bump = bump_of("Update dependency left-pad to v1.3.4", TABLE).unwrap();
        assert_eq!(bump.from.as_deref(), Some("1.2.0"));
        assert_eq!(bump.kind, UpdateKind::Minor);
        let no_body = bump_of("Update dependency left-pad to v1.3.4", "").unwrap();
        assert_eq!(no_body.kind, UpdateKind::Unknown);
        let hinted = bump_of("Update dependency left-pad to v2 (major)", "").unwrap();
        assert_eq!(hinted.kind, UpdateKind::Major);
        assert!(bump_of("Add feature", TABLE).is_none());
        let build_only = bump_of(
            "Update dependency java-jdk to v25.0.5+100000002.0.LTS (patch)",
            "| java-jdk | `25.0.5+8.0.LTS` -> `25.0.5+100000002.0.LTS` |",
        )
        .unwrap();
        assert_eq!(build_only.kind, UpdateKind::Unknown);
    }

    fn bump(title: &str) -> Bump {
        parse_bump(title).unwrap_or_else(|| panic!("no bump in {title:?}"))
    }

    fn renovate(title: &str, dependency: &str, to: &str) {
        let parsed = bump(title);
        assert_eq!(parsed.dependency, dependency, "{title}");
        assert_eq!(parsed.to, to, "{title}");
        assert_eq!(parsed.from, None, "{title}");
        assert_eq!(parsed.kind, UpdateKind::Unknown, "{title}");
    }

    #[test]
    fn renovate_titles() {
        renovate(
            "Update dependency fast-xml-parser to v5.12.1",
            "fast-xml-parser",
            "5.12.1",
        );
        renovate("Update Gradle to v9.9.2", "Gradle", "9.9.2");
        renovate(
            "Update plugin org.springframework.boot to v4.2.0",
            "org.springframework.boot",
            "4.2.0",
        );
        renovate(
            "Update dependency org.apache.maven.plugins:maven-surefire-plugin to v3.6.1",
            "org.apache.maven.plugins:maven-surefire-plugin",
            "3.6.1",
        );
        renovate(
            "Update trixnity.version to v5.11.0",
            "trixnity.version",
            "5.11.0",
        );
        renovate(
            "Update dependency java-jdk to v25.0.5+100000002.0.LTS",
            "java-jdk",
            "25.0.5+100000002.0.LTS",
        );
        renovate(
            "Update dependency com.diffplug.spotless to v8.11.0",
            "com.diffplug.spotless",
            "8.11.0",
        );
    }

    #[test]
    fn renovate_with_prefix_and_kind_hint() {
        let parsed = bump("Update dependency vite to v5 (major)");
        assert_eq!(
            (parsed.dependency.as_str(), parsed.to.as_str()),
            ("vite", "5")
        );
        assert_eq!(parsed.kind, UpdateKind::Major);
        assert_eq!(
            bump("chore(deps): update dependency x to v1.2 (minor)").kind,
            UpdateKind::Minor
        );
        assert_eq!(bump("Update x to 1.0.1 (patch)").kind, UpdateKind::Patch);
        assert_eq!(bump("Update x to v1 (weird)").kind, UpdateKind::Unknown);
    }

    #[test]
    fn dependabot_titles() {
        let cases = [
            (
                "Dependency(deps): Bump junit:junit from 3.8.1 to 4.13.2",
                "junit:junit",
                "3.8.1",
                "4.13.2",
                UpdateKind::Major,
            ),
            (
                "Bump com.example:lib from 0.22.14 to 0.22.15",
                "com.example:lib",
                "0.22.14",
                "0.22.15",
                UpdateKind::Patch,
            ),
            (
                "Dependency(deps-dev): Bump eslint from 10.12.0 to 10.14.0",
                "eslint",
                "10.12.0",
                "10.14.0",
                UpdateKind::Minor,
            ),
            (
                "Bump maven from 3-eclipse-temurin-25 to 3-eclipse-temurin-27",
                "maven",
                "3-eclipse-temurin-25",
                "3-eclipse-temurin-27",
                UpdateKind::Unknown,
            ),
            (
                "Bump serde from 1.0.1 to 1.0.2 in /subdir",
                "serde",
                "1.0.1",
                "1.0.2",
                UpdateKind::Patch,
            ),
            (
                "chore(deps): bump x from v1.2.3 to v2.0.0",
                "x",
                "1.2.3",
                "2.0.0",
                UpdateKind::Major,
            ),
            (
                "Bump org.commonmark:commonmark from 0.31.0 to 0.33.0",
                "org.commonmark:commonmark",
                "0.31.0",
                "0.33.0",
                UpdateKind::Minor,
            ),
        ];
        for (title, dependency, from, to, kind) in cases {
            let parsed = bump(title);
            assert_eq!(parsed.dependency, dependency, "{title}");
            assert_eq!(parsed.from.as_deref(), Some(from), "{title}");
            assert_eq!(parsed.to, to, "{title}");
            assert_eq!(parsed.kind, kind, "{title}");
        }
    }

    #[test]
    fn non_bumps() {
        for title in [
            "Bump react and @types/react",
            "Bump the npm group with 3 updates",
            "Fix the build",
            "Update README",
            "Bump  from 1 to 2",
            "",
            "Update x to ",
        ] {
            assert_eq!(parse_bump(title), None, "{title:?}");
        }
    }

    #[test]
    fn classification() {
        assert_eq!(classify("1.2.3", "1.2.4"), UpdateKind::Patch);
        assert_eq!(classify("1.2.3", "1.3.0"), UpdateKind::Minor);
        assert_eq!(classify("1.2.3", "2.0.0"), UpdateKind::Major);
        assert_eq!(classify("v1", "v2"), UpdateKind::Major);
        assert_eq!(classify("0.31", "0.33"), UpdateKind::Minor);
        assert_eq!(classify("0.0.1", "0.0.2"), UpdateKind::Patch);
        assert_eq!(classify("1.0.0.1", "1.0.0.2"), UpdateKind::Patch);
        assert_eq!(classify("1.0.0", "1.0.0"), UpdateKind::Unknown);
        assert_eq!(classify("1.0.0", "1.0.x"), UpdateKind::Unknown);
        assert_eq!(classify("abc", "abd"), UpdateKind::Unknown);
        assert_eq!(
            classify("25.0.5+8.0.LTS", "25.0.5+100000002.0.LTS"),
            UpdateKind::Unknown
        );
        assert_eq!(classify("1.2.3+1", "1.2.4+1"), UpdateKind::Patch);
        assert_eq!(classify("1.2.3+9", "1.3.0"), UpdateKind::Minor);
        assert_eq!(classify("1.0.0-rc.1", "1.0.0"), UpdateKind::Patch);
    }

    #[test]
    fn same_version_written_two_ways_is_no_update() {
        assert_eq!(classify("1.2", "1.2.0"), UpdateKind::Unknown);
        assert_eq!(classify("v1.2.0", "1.2"), UpdateKind::Unknown);
        assert_eq!(classify("1.2.0+1", "1.2"), UpdateKind::Unknown);
        let bump = bump_of(
            "Update dependency x to v1.2.0 (patch)",
            "| x | `1.2` -> `1.2.0` |",
        )
        .unwrap();
        assert_eq!(bump.kind, UpdateKind::Unknown);
    }

    #[test]
    fn dependabot_directory() {
        let sub = bump("Bump serde from 1.0.1 to 1.0.2 in /subdir");
        assert_eq!(sub.directory.as_deref(), Some("/subdir"));
        assert_eq!(bump("Bump serde from 1.0.1 to 1.0.2 in /").directory, None);
        assert_eq!(bump("Bump serde from 1.0.1 to 1.0.2").directory, None);
    }

    #[test]
    fn version_order() {
        assert_eq!(compare_versions("0.33.0", "0.32.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.9", "1.10"), Ordering::Less);
        assert_eq!(compare_versions("v2", "2.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("3.6.1", "3.6.1"), Ordering::Equal);
        assert_eq!(compare_versions("1.0.0", "1.0.0-beta"), Ordering::Greater);
    }
}
