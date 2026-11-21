//! The rebase action: Dependabot by comment, Renovate by its checkbox, everyone else through
//! GitHub's branch update.

use crate::client::Client;
use crate::error::{Error, Result};
use crate::mutations::node_id;
use protocol::PrItem;
use serde_json::json;

const DEPENDABOT: &str = "dependabot[bot]";
const RENOVATE: &str = "renovate[bot]";
const MARKER: &str = "rebase-check";

/// The current description (the cached one may be outdated).
const BODY_QUERY: &str = "query($id: ID!) { node(id: $id) { ... on PullRequest { body } } }";

/// Why the Renovate checkbox could not be ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickError {
    Missing,
    AlreadyChecked,
}

/// Whether the line is the Renovate rebase checkbox, and if so whether it is ticked.
/// Tolerates indentation, `*` bullets, `[X]` and whitespace inside the HTML comment.
fn checkbox_state(line: &str) -> Option<bool> {
    let rest = line.trim_start().strip_prefix(['-', '*'])?.trim_start();
    let (checked, rest) = if let Some(rest) = rest.strip_prefix("[ ]") {
        (false, rest)
    } else {
        (
            true,
            rest.strip_prefix("[x]")
                .or_else(|| rest.strip_prefix("[X]"))?,
        )
    };
    let comment = rest.trim_start().strip_prefix("<!--")?;
    let (inner, _) = comment.split_once("-->")?;
    (inner.trim() == MARKER).then_some(checked)
}

/// Ticks the unchecked `- [ ] <!-- rebase-check -->` box of a Renovate PR body.
fn tick_rebase_checkbox(body: &str) -> std::result::Result<String, TickError> {
    let mut found_checked = false;
    let mut out = String::with_capacity(body.len());
    let mut ticked = false;
    for line in body.split_inclusive('\n') {
        match checkbox_state(line) {
            Some(false) if !ticked => {
                out.push_str(&line.replacen("[ ]", "[x]", 1));
                ticked = true;
            }
            Some(true) => {
                found_checked = true;
                out.push_str(line);
            }
            _ => out.push_str(line),
        }
    }
    if ticked {
        Ok(out)
    } else if found_checked {
        Err(TickError::AlreadyChecked)
    } else {
        Err(TickError::Missing)
    }
}

impl Client {
    /// Rebases by the author of the PR: Dependabot by comment, Renovate by its checkbox,
    /// everyone else through GitHub's branch update (pinned to the head).
    pub(crate) async fn rebase_item(&self, item: &PrItem) -> Result<String> {
        match item.author.as_str() {
            DEPENDABOT => {
                self.comment(item, "@dependabot rebase").await?;
                Ok("asked Dependabot to rebase".to_owned())
            }
            RENOVATE => {
                self.renovate_rebase(item).await?;
                Ok("asked Renovate to rebase".to_owned())
            }
            _ => {
                self.update_branch(item).await?;
                Ok("branch update (rebase) requested".to_owned())
            }
        }
    }

    async fn renovate_rebase(&self, item: &PrItem) -> Result<()> {
        let id = node_id(item)?;
        let data = self.graphql(BODY_QUERY, json!({ "id": id })).await?;
        let body = data["node"]["body"]
            .as_str()
            .ok_or_else(|| Error::Parse(format!("{}: pull request not found", item.pr)))?;
        let updated = tick_rebase_checkbox(body).map_err(|error| {
            Error::Refused(match error {
                TickError::Missing => "no rebase checkbox in the Renovate PR body".to_owned(),
                TickError::AlreadyChecked => "rebase already requested".to_owned(),
            })
        })?;
        self.update_body(item, &updated).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_the_checkbox_and_keeps_the_rest() {
        let body = "intro\r\n\r\n- [ ] <!-- rebase-check -->If you want to rebase, check this box\r\n\r\n---\r\nmore";
        assert_eq!(
            tick_rebase_checkbox(body).unwrap(),
            "intro\r\n\r\n- [x] <!-- rebase-check -->If you want to rebase, check this box\r\n\r\n---\r\nmore"
        );
    }

    #[test]
    fn tolerates_whitespace_variants() {
        for line in [
            "-   [ ]   <!--rebase-check-->x",
            "  * [ ] <!--  rebase-check  --> x",
            "- [ ]<!-- rebase-check -->",
        ] {
            let ticked = tick_rebase_checkbox(line).unwrap();
            assert!(
                ticked.contains("[x]") && !ticked.contains("[ ]"),
                "{ticked}"
            );
            assert_eq!(ticked.len(), line.len());
        }
    }

    #[test]
    fn reports_missing_and_checked() {
        assert_eq!(tick_rebase_checkbox("nothing"), Err(TickError::Missing));
        assert_eq!(
            tick_rebase_checkbox("- [ ] <!-- other-check -->x"),
            Err(TickError::Missing)
        );
        assert_eq!(
            tick_rebase_checkbox("- [x] <!-- rebase-check -->x"),
            Err(TickError::AlreadyChecked)
        );
        assert_eq!(
            tick_rebase_checkbox("- [X] <!-- rebase-check -->x"),
            Err(TickError::AlreadyChecked)
        );
    }
}
