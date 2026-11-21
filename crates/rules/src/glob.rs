//! A tiny glob matcher for `owner/name` repository patterns.

/// Matches `text` against `pattern`, case-insensitively, over `/`-separated segments.
///
/// - `*` matches any run of characters inside one segment (`acme/*`, `*/widget`, `a/pre*`).
/// - `?` matches one character inside a segment.
/// - a segment that is exactly `**` matches any number of whole segments (including none).
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();
    let pattern: Vec<&str> = pattern.split('/').collect();
    let text: Vec<&str> = text.split('/').collect();
    segments_match(&pattern, &text)
}

/// Why `pattern` can never match an `owner/name` repository (empty, or without the slash);
/// `None` when it can.
pub fn glob_problem(pattern: &str) -> Option<&'static str> {
    (pattern.trim().is_empty() || (!pattern.contains('/') && pattern != "**"))
        .then_some("never matches: use owner/name globs like `owner/*`")
}

fn segments_match(pattern: &[&str], text: &[&str]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((&"**", rest)) => (0..=text.len()).any(|skip| segments_match(rest, &text[skip..])),
        Some((first, rest)) => match text.split_first() {
            Some((segment, text_rest)) => {
                wildcard_match(first, segment) && segments_match(rest, text_rest)
            }
            None => false,
        },
    }
}

/// `*` and `?` matching within one segment (iterative, with backtracking to the last `*`).
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            backtrack = Some((p, t));
            p += 1;
        } else if let Some((star, matched)) = backtrack {
            p = star + 1;
            t = matched + 1;
            backtrack = Some((star, matched + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::{glob_match, glob_problem};

    #[test]
    fn literal_and_case() {
        assert!(glob_match("acme/widget", "acme/widget"));
        assert!(glob_match("Acme/Widget", "acme/widget"));
        assert!(!glob_match("acme/widget", "acme/widget2"));
    }

    #[test]
    fn star_stays_within_a_segment() {
        assert!(glob_match("acme/*", "acme/widget"));
        assert!(!glob_match("acme/*", "other/widget"));
        assert!(!glob_match("acme/*", "acme/a/b"));
        assert!(glob_match("*/widget", "x/widget"));
        assert!(glob_match("acme/widget-*", "acme/widget-extra"));
        assert!(!glob_match("acme/widget-*", "acme/bot-widget"));
        assert!(glob_match("*/*", "a/b"));
        assert!(glob_match("a/*b*c", "a/xxbyyc"));
        assert!(!glob_match("a/*b*c", "a/xxbyy"));
        assert!(glob_match("a/*", "a/"));
    }

    #[test]
    fn question_mark() {
        assert!(glob_match("a/b?", "a/bc"));
        assert!(!glob_match("a/b?", "a/b"));
    }

    #[test]
    fn double_star_spans_segments() {
        assert!(glob_match("**", "a/b"));
        assert!(glob_match("a/**", "a/b"));
        assert!(glob_match("a/**", "a/b/c"));
        assert!(glob_match("a/**", "a"));
        assert!(glob_match("**/c", "a/b/c"));
        assert!(!glob_match("a/**", "b/c"));
    }

    #[test]
    fn useless_patterns_are_named() {
        for useless in ["", "  ", "acme", "*"] {
            assert!(glob_problem(useless).is_some(), "{useless:?}");
        }
        for fine in ["acme/*", "*/x", "**", "a/**"] {
            assert_eq!(glob_problem(fine), None, "{fine:?}");
        }
    }

    #[test]
    fn single_star_is_not_everything() {
        assert!(!glob_match("*", "a/b"));
        assert!(glob_match("*", "a"));
    }
}
