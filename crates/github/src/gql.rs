//! GraphQL helpers shared by the queries: quoting and cursor pagination.

use crate::client::Client;
use crate::error::Result;
use serde_json::Value;

/// A GraphQL string literal (JSON quoting is valid GraphQL quoting).
pub(crate) fn quote(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// The owner and name of an `owner/name` repository; the name is empty without a slash.
pub(crate) fn owner_and_name(repo: &str) -> (&str, &str) {
    repo.split_once('/').unwrap_or((repo, ""))
}

/// The `endCursor` of a `pageInfo` when there is a next page.
pub(crate) fn next_cursor(page_info: &Value) -> Option<String> {
    page_info["hasNextPage"]
        .as_bool()
        .unwrap_or(false)
        .then(|| page_info["endCursor"].as_str().map(str::to_owned))
        .flatten()
}

impl Client {
    /// Runs `query` page by page, passing the cursor as `$after`, until `page` reports no next
    /// cursor. Partial errors are tolerated like in [`Client::graphql_partial`].
    pub(crate) async fn paginate<T>(
        &self,
        query: &str,
        mut variables: Value,
        page: impl Fn(&Value) -> (Vec<T>, Option<String>),
    ) -> Result<Vec<T>> {
        let mut all = Vec::new();
        loop {
            let data = self.graphql_partial(query, variables.clone()).await?;
            let (found, next) = page(&data);
            all.extend(found);
            match next {
                Some(cursor) => variables["after"] = Value::String(cursor),
                None => return Ok(all),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quotes_and_cursors() {
        assert_eq!(quote("ac\"me"), "\"ac\\\"me\"");
        assert_eq!(owner_and_name("acme/widget"), ("acme", "widget"));
        assert_eq!(owner_and_name("solo"), ("solo", ""));
        let more = json!({"hasNextPage": true, "endCursor": "C"});
        assert_eq!(next_cursor(&more).as_deref(), Some("C"));
        let last = json!({"hasNextPage": false, "endCursor": "C"});
        assert_eq!(next_cursor(&last), None);
    }
}
