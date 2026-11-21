//! Notification threads (REST): listing the pull-request threads and marking them.

use crate::client::{Client, header, read_json};
use crate::error::{Error, Result};
use crate::mapping::notify_reason;
use protocol::{NotificationInfo, PrRef};
use reqwest::Method;
use serde_json::Value;

/// A pull-request notification thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotificationThread {
    pub pr: PrRef,
    pub info: NotificationInfo,
}

impl Client {
    /// All pull-request notification threads (`all`: read ones too), every page. Remembers the
    /// `X-Poll-Interval` GitHub asks for.
    pub(crate) async fn notifications(&self, all: bool) -> Result<Vec<NotificationThread>> {
        let mut threads = Vec::new();
        let mut response = self
            .rest(
                Method::GET,
                &format!("/notifications?all={all}&per_page=50"),
            )
            .await?;
        if let Some(secs) = header(&response, "x-poll-interval").and_then(|v| v.parse().ok()) {
            self.remember_poll_interval(secs);
        }
        loop {
            let next = header(&response, "link").and_then(|link| parse_next_link(&link));
            threads.extend(parse_notifications(&read_json(response).await?));
            match next {
                None => return Ok(threads),
                Some(url) if self.is_own_url(&url) => response = self.get_url(url).await?,
                Some(url) => {
                    return Err(Error::Parse(format!(
                        "notifications page link outside the API: {url}"
                    )));
                }
            }
        }
    }

    pub(crate) async fn mark_read(&self, thread_id: &str) -> Result<()> {
        let path = format!("/notifications/threads/{thread_id}");
        self.rest(Method::PATCH, &path).await.map(drop)
    }

    pub(crate) async fn mark_done(&self, thread_id: &str) -> Result<()> {
        let path = format!("/notifications/threads/{thread_id}");
        self.rest(Method::DELETE, &path).await.map(drop)
    }

    pub(crate) async fn unsubscribe(&self, thread_id: &str) -> Result<()> {
        let path = format!("/notifications/threads/{thread_id}/subscription");
        self.rest(Method::DELETE, &path).await.map(drop)
    }
}

/// Extracts `owner/repo` and the number from `https://api.github.com/repos/o/r/pulls/12`.
fn parse_subject_url(url: &str) -> Option<PrRef> {
    let (_, rest) = url.split_once("/repos/")?;
    let mut parts = rest.split('/');
    let (owner, name, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if kind != "pulls" || owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(PrRef {
        repo: format!("{owner}/{name}"),
        number: number.parse().ok()?,
    })
}

/// Keeps the `PullRequest` threads of one notifications page (a JSON array).
fn parse_notifications(page: &Value) -> Vec<NotificationThread> {
    page.as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_thread)
        .collect()
}

fn parse_thread(thread: &Value) -> Option<NotificationThread> {
    let subject = &thread["subject"];
    if subject["type"].as_str()? != "PullRequest" {
        return None;
    }
    let pr = parse_subject_url(subject["url"].as_str()?)?;
    Some(NotificationThread {
        pr,
        info: NotificationInfo {
            thread_id: thread["id"].as_str()?.to_owned(),
            reason: notify_reason(thread["reason"].as_str().unwrap_or_default()),
            unread: thread["unread"].as_bool().unwrap_or(false),
            updated_at: thread["updated_at"].as_str().unwrap_or_default().to_owned(),
        },
    })
}

/// The `rel="next"` URL of a `Link` header.
fn parse_next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        let (url, params) = part.split_once(';')?;
        let is_next = params.split(';').any(|p| p.trim() == "rel=\"next\"");
        let url = url.trim().strip_prefix('<')?.strip_suffix('>')?;
        is_next.then(|| url.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::NotifyReason;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/notifications.json")).unwrap()
    }

    #[test]
    fn keeps_only_pull_requests() {
        let threads = parse_notifications(&fixture());
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].pr.repo, "acme/widget");
        assert_eq!(threads[0].pr.number, 12);
        assert_eq!(threads[0].info.thread_id, "1001");
        assert_eq!(threads[0].info.reason, NotifyReason::ReviewRequested);
        assert!(threads[0].info.unread);
        assert!(!threads[1].info.unread);
        assert_eq!(threads[1].info.reason, NotifyReason::Mention);
    }

    #[test]
    fn subject_urls() {
        let pr = parse_subject_url("https://api.github.com/repos/acme/widget/pulls/7").unwrap();
        assert_eq!((pr.repo.as_str(), pr.number), ("acme/widget", 7));
        assert!(parse_subject_url("https://api.github.com/repos/acme/widget/issues/7").is_none());
        assert!(parse_subject_url("https://example.com/x").is_none());
    }

    #[test]
    fn next_link() {
        let header = "<https://api.github.com/notifications?page=2>; rel=\"next\", \
                      <https://api.github.com/notifications?page=5>; rel=\"last\"";
        assert_eq!(
            parse_next_link(header).as_deref(),
            Some("https://api.github.com/notifications?page=2")
        );
        let last = "<https://api.github.com/notifications?page=1>; rel=\"prev\"";
        assert_eq!(parse_next_link(last), None);
    }
}
