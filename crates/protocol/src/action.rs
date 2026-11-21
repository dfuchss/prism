//! Actions on pull requests, their results and the audit trail.

use crate::inbox::{MergeMethod, PrRef};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Something Prism can do to a pull request or its notification.
///
/// The JSON form (externally tagged, variant names) is stored in `audit.jsonl` and must stay
/// readable. `rules.toml` uses the string form of [`fmt::Display`] and [`FromStr`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Approve,
    /// `None` uses the repository's preferred allowed method.
    Merge(Option<MergeMethod>),
    ApproveAndMerge(Option<MergeMethod>),
    EnableAutoMerge(Option<MergeMethod>),
    Close {
        comment: Option<String>,
    },
    Comment(String),
    MarkRead,
    MarkDone,
    Unsubscribe,
    RerunFailed,
    /// Brings the branch up to date: a bot rebase request or GitHub's branch update.
    Rebase,
}

/// The merge methods in their rule-file spelling.
const METHOD_NAMES: [(MergeMethod, &str); 3] = [
    (MergeMethod::Merge, "merge"),
    (MergeMethod::Squash, "squash"),
    (MergeMethod::Rebase, "rebase"),
];

impl Action {
    /// Only touches the notification thread (mark read, mark done, unsubscribe); the only
    /// actions allowed on merged and closed pull requests.
    pub fn is_notification_only(&self) -> bool {
        matches!(self, Self::MarkRead | Self::MarkDone | Self::Unsubscribe)
    }
}

/// The rule-file form: `approve`, `merge[:method]`, `approve-and-merge[:method]`,
/// `auto-merge[:method]`, `close[:"comment"]`, `comment:"text"`, `mark-read`, `mark-done`,
/// `unsubscribe`, `rerun-failed`, `rebase`. Texts are always quoted; an empty closing comment
/// is left out.
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let with_method = |f: &mut fmt::Formatter<'_>, name: &str, method: &Option<MergeMethod>| {
            let spelled = METHOD_NAMES.iter().find(|(m, _)| Some(*m) == *method);
            match spelled {
                Some((_, method)) => write!(f, "{name}:{method}"),
                None => f.write_str(name),
            }
        };
        match self {
            Self::Approve => f.write_str("approve"),
            Self::Merge(method) => with_method(f, "merge", method),
            Self::ApproveAndMerge(method) => with_method(f, "approve-and-merge", method),
            Self::EnableAutoMerge(method) => with_method(f, "auto-merge", method),
            Self::Close { comment } => match comment.as_deref() {
                None | Some("") => f.write_str("close"),
                Some(comment) => write!(f, "close:\"{comment}\""),
            },
            Self::Comment(comment) => write!(f, "comment:\"{comment}\""),
            Self::MarkRead => f.write_str("mark-read"),
            Self::MarkDone => f.write_str("mark-done"),
            Self::Unsubscribe => f.write_str("unsubscribe"),
            Self::RerunFailed => f.write_str("rerun-failed"),
            Self::Rebase => f.write_str("rebase"),
        }
    }
}

/// Parses the rule-file form (see [`fmt::Display`]). Whitespace around the name and the
/// argument is ignored, quotes around texts are optional, and the text is everything after
/// the first `:` (it may contain colons and quotes). `comment` needs a non-empty text.
impl FromStr for Action {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (name, argument) = match text.split_once(':') {
            Some((name, argument)) => (name.trim(), Some(argument.trim())),
            None => (text, None),
        };
        let method = || match argument {
            None | Some("") => Ok(None),
            Some(spelled) => METHOD_NAMES
                .iter()
                .find(|(_, name)| *name == spelled)
                .map(|(method, _)| Some(*method))
                .ok_or_else(|| format!("unknown merge method `{spelled}`")),
        };
        let text = argument.map(unquote).filter(|text| !text.is_empty());
        let no_argument = |action: Self| match argument {
            None => Ok(action),
            Some(_) => Err(format!("action `{name}` takes no argument")),
        };
        match name {
            "approve" => no_argument(Self::Approve),
            "merge" => method().map(Self::Merge),
            "approve-and-merge" => method().map(Self::ApproveAndMerge),
            "auto-merge" => method().map(Self::EnableAutoMerge),
            "close" => Ok(Self::Close { comment: text }),
            "comment" => text
                .map(Self::Comment)
                .ok_or_else(|| "action `comment` needs a text".to_owned()),
            "mark-read" => no_argument(Self::MarkRead),
            "mark-done" => no_argument(Self::MarkDone),
            "unsubscribe" => no_argument(Self::Unsubscribe),
            "rerun-failed" => no_argument(Self::RerunFailed),
            "rebase" => no_argument(Self::Rebase),
            other => Err(format!("unknown action `{other}`")),
        }
    }
}

/// Removes one pair of surrounding double quotes.
fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    pub pr: PrRef,
    pub action: Action,
    pub ok: bool,
    pub message: String,
}

/// Who started a batch of actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProgressSource {
    Manual,
    Planned,
    Auto,
}

/// Progress of a running batch of actions (the backend event `action-progress`). Sent once when
/// the batch starts, once when each PR starts and once after each result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionProgress {
    /// Identifies the batch; grows with every batch.
    pub batch_id: u64,
    pub total: usize,
    /// Jobs finished so far (reported or silently skipped).
    pub done: usize,
    /// The PR being worked on; `None` when the batch starts.
    pub current: Option<PrRef>,
    /// The action of the current job (of the first job when the batch starts).
    pub action: Action,
    /// The result that just came in, if this event reports one.
    pub last: Option<ActionResult>,
    pub source: ProgressSource,
}

/// A failed job of a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedJob {
    pub name: String,
    pub html_url: String,
}

/// An action a rule proposes (or, for `auto` rules, is about to run).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedAction {
    /// Name of the rule, for display and the audit log.
    pub rule: String,
    /// Position of the rule in the list the plan was made from. The executor looks the rule up
    /// by position and checks the name, so rules sharing a name are never mixed up.
    pub rule_index: usize,
    pub pr: PrRef,
    pub action: Action,
    /// Head commit the plan was made for; head-bound actions are refused once it changed.
    pub head_oid: String,
}

/// A pull request a manual action is for, with the head commit the user saw (`None`: unknown;
/// the cached one counts). Approvals and merges are refused once the head moved on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActTarget {
    pub pr: PrRef,
    pub head: Option<String>,
}

/// One executed action, as a line of `audit.jsonl`: `at` and `rule` plus the fields of the
/// [`ActionResult`] (`pr`, `action`, `ok`, `message`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub at: String,
    /// `None` for actions the user triggered manually.
    pub rule: Option<String>,
    #[serde(flatten)]
    pub result: ActionResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Action {
        text.parse()
            .unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    #[test]
    fn rule_strings_roundtrip() {
        for text in [
            "approve",
            "merge",
            "merge:squash",
            "merge:merge",
            "approve-and-merge:rebase",
            "auto-merge",
            "close",
            "close:\"superseded\"",
            "comment:\"thanks\"",
            "mark-read",
            "mark-done",
            "unsubscribe",
            "rerun-failed",
            "rebase",
        ] {
            assert_eq!(parse(text).to_string(), text);
        }
    }

    #[test]
    fn texts_keep_colons_quotes_and_inner_whitespace() {
        for comment in [
            "a: b",
            "say \"hi\"",
            "\"quoted\"",
            " padded ",
            "\"",
            "x:\"y\"",
        ] {
            let action = Action::Comment(comment.to_owned());
            assert_eq!(parse(&action.to_string()), action, "{comment:?}");
            let close = Action::Close {
                comment: Some(comment.to_owned()),
            };
            assert_eq!(parse(&close.to_string()), close, "{comment:?}");
        }
        assert_eq!(
            parse(" comment : hello "),
            Action::Comment("hello".to_owned())
        );
        assert_eq!(parse("close:\"\""), Action::Close { comment: None });
        let empty = Action::Close {
            comment: Some(String::new()),
        };
        assert_eq!(empty.to_string(), "close");
    }

    #[test]
    fn rule_strings_reject_garbage() {
        for text in [
            "explode",
            "approve:now",
            "merge:fast",
            "comment",
            "comment:\"\"",
            "rebase:now",
            "",
        ] {
            assert!(text.parse::<Action>().is_err(), "{text:?}");
        }
    }

    #[test]
    fn json_roundtrips() {
        let pr = PrRef {
            repo: "acme/widget".to_owned(),
            number: 12,
        };
        let result = ActionResult {
            pr: pr.clone(),
            action: Action::Close {
                comment: Some("dup".to_owned()),
            },
            ok: false,
            message: "nope".to_owned(),
        };
        let progress = ActionProgress {
            batch_id: 3,
            total: 10,
            done: 4,
            current: Some(pr),
            action: Action::Merge(None),
            last: Some(result.clone()),
            source: ProgressSource::Auto,
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert_eq!(
            serde_json::from_str::<ActionProgress>(&json).unwrap(),
            progress
        );
    }

    #[test]
    fn existing_audit_lines_still_read_and_are_written_the_same() {
        let line = r#"{"at":"2026-11-21T10:00:00Z","rule":"bumps","pr":{"repo":"acme/widget","number":7},"action":{"Merge":"Squash"},"ok":true,"message":"merged"}"#;
        let entry: AuditEntry = serde_json::from_str(line).unwrap();
        assert_eq!(entry.rule.as_deref(), Some("bumps"));
        assert_eq!(
            entry.result.action,
            Action::Merge(Some(MergeMethod::Squash))
        );
        assert!(entry.result.ok);
        assert_eq!(serde_json::to_string(&entry).unwrap(), line);
        for (action, expected) in [
            (
                r#"{"Close":{"comment":null}}"#,
                Action::Close { comment: None },
            ),
            (r#"{"ApproveAndMerge":null}"#, Action::ApproveAndMerge(None)),
            (r#""MarkRead""#, Action::MarkRead),
            (
                r#"{"Comment":"thanks"}"#,
                Action::Comment("thanks".to_owned()),
            ),
        ] {
            let manual = format!(
                r#"{{"at":"t","rule":null,"pr":{{"repo":"a/b","number":1}},"action":{action},"ok":false,"message":"x"}}"#
            );
            let entry: AuditEntry = serde_json::from_str(&manual).unwrap();
            assert_eq!(entry.result.action, expected);
            assert_eq!(serde_json::to_string(&entry).unwrap(), manual);
        }
    }
}
