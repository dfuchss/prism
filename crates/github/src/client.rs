//! The transport: authenticated requests with one retry after a 401, rate-limit tracking,
//! error mapping and the REST and GraphQL entry points. The domain modules add their own
//! `impl Client` blocks.

use crate::error::{Error, Result};
use crate::lock;
use crate::token::TokenSource;
use reqwest::header::HeaderMap;
use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_BASE_URL: &str = "https://api.github.com";
/// Wait after a 429 that names no time.
const DEFAULT_RATE_LIMIT_SECS: u64 = 60;
/// Longest excerpt of a non-JSON error body shown to the user.
const MAX_ERROR_TEXT: usize = 200;

/// GitHub API client (REST and GraphQL).
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    tokens: TokenSource,
    viewer: Mutex<Option<String>>,
    /// Last `X-Poll-Interval` of the notifications API; 0 = not seen yet.
    poll_interval: AtomicU64,
    /// Unix time until which GitHub asked to wait (rate limit); 0 = never.
    rate_limited_until: AtomicU64,
}

struct Request<'a> {
    method: Method,
    url: String,
    body: Option<&'a Value>,
}

impl Client {
    /// Talks to `https://api.github.com` with the token from env vars or `gh`.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL, TokenSource::new())
    }

    /// Custom API root (without trailing slash) and token source, for tests.
    pub(crate) fn with_base_url(base_url: &str, tokens: TokenSource) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            tokens,
            viewer: Mutex::new(None),
            poll_interval: AtomicU64::new(0),
            rate_limited_until: AtomicU64::new(0),
        }
    }

    /// Sets the `gh` executable used for `gh auth token` (empty: `gh` from `PATH`) and forgets
    /// the cached token.
    pub fn set_gh_path(&self, path: &str) {
        self.tokens.set_gh_path(path);
    }

    /// Where the token comes from (never the token itself).
    pub fn auth_source(&self) -> String {
        self.tokens.describe()
    }

    /// Seconds until the next poll as far as GitHub is concerned: the `X-Poll-Interval` of the
    /// last notifications response, or longer while a rate limit lasts. `None`: no preference.
    pub fn poll_interval_secs(&self) -> Option<u64> {
        let interval = self.poll_interval.load(Ordering::Relaxed);
        let wait = self
            .rate_limited_until
            .load(Ordering::Relaxed)
            .saturating_sub(unix_now());
        Some(interval.max(wait)).filter(|secs| *secs > 0)
    }

    pub(crate) fn remember_poll_interval(&self, secs: u64) {
        self.poll_interval.store(secs, Ordering::Relaxed);
    }

    /// Whether `url` lies below the API root (pagination links are only followed there, so the
    /// token never goes elsewhere).
    pub(crate) fn is_own_url(&self, url: &str) -> bool {
        url.strip_prefix(&self.base_url)
            .is_some_and(|rest| rest.starts_with('/'))
    }

    /// Sends the request; on a 401 the token is invalidated, refetched and the request retried
    /// once. Any other non-success status is an error.
    async fn send(&self, request: &Request<'_>) -> Result<Response> {
        for attempt in 0..2 {
            let token = self.tokens.token().await?;
            let response = self.build(request, &token).send().await?;
            if response.status() != StatusCode::UNAUTHORIZED {
                return self.check_status(response).await;
            }
            self.tokens.invalidate();
            if attempt == 1 {
                break;
            }
        }
        Err(Error::Unauthorized)
    }

    fn build(&self, request: &Request<'_>, token: &str) -> reqwest::RequestBuilder {
        let builder = self
            .http
            .request(request.method.clone(), &request.url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "prism");
        match request.body {
            Some(body) => builder.json(body),
            None => builder,
        }
    }

    /// A GET of an absolute URL below the API root (a pagination link).
    pub(crate) async fn get_url(&self, url: String) -> Result<Response> {
        let request = Request {
            method: Method::GET,
            url,
            body: None,
        };
        self.send(&request).await
    }

    /// A REST call with a path below the API root.
    pub(crate) async fn rest(&self, method: Method, path: &str) -> Result<Response> {
        let request = Request {
            method,
            url: format!("{}{path}", self.base_url),
            body: None,
        };
        self.send(&request).await
    }

    /// Runs a GraphQL operation and returns its `data`; any GraphQL error fails it.
    pub(crate) async fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        extract_data(self.graphql_response(query, variables).await?, true)
    }

    /// Like [`Client::graphql`], but errors are tolerated while data is present: a pull request
    /// or repository GitHub will not show (e.g. SAML enforcement) is null plus an error and
    /// must not fail the whole query.
    pub(crate) async fn graphql_partial(&self, query: &str, variables: Value) -> Result<Value> {
        extract_data(self.graphql_response(query, variables).await?, false)
    }

    async fn graphql_response(&self, query: &str, variables: Value) -> Result<Value> {
        let body = json!({ "query": query, "variables": variables });
        let request = Request {
            method: Method::POST,
            url: format!("{}/graphql", self.base_url),
            body: Some(&body),
        };
        read_json(self.send(&request).await?).await
    }

    /// Login of the authenticated user (fetched once, then cached).
    pub async fn viewer_login(&self) -> Result<String> {
        if let Some(login) = lock(&self.viewer).clone() {
            return Ok(login);
        }
        let data = self
            .graphql("query { viewer { login } }", json!({}))
            .await?;
        let login = data["viewer"]["login"]
            .as_str()
            .ok_or_else(|| Error::Parse("viewer login missing".to_owned()))?
            .to_owned();
        *lock(&self.viewer) = Some(login.clone());
        Ok(login)
    }

    /// Passes successful responses; turns the others into errors and remembers rate limits.
    async fn check_status(&self, response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if let Some(retry_after_secs) = rate_limit_wait(status, response.headers(), unix_now()) {
            self.rate_limited_until
                .store(unix_now() + retry_after_secs, Ordering::Relaxed);
            return Err(Error::RateLimited { retry_after_secs });
        }
        let body = response.text().await.unwrap_or_default();
        Err(Error::Http {
            status: status.as_u16(),
            message: error_message(&body, status),
        })
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Reads a JSON body; a body that is no JSON is a [`Error::Parse`].
pub(crate) async fn read_json(response: Response) -> Result<Value> {
    let bytes = response.bytes().await?;
    serde_json::from_slice(&bytes).map_err(|error| Error::Parse(format!("invalid JSON: {error}")))
}

/// The value of a response header, if it is text.
pub(crate) fn header(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

/// How long GitHub asks to wait, for a 429 or a 403 that is a rate limit: `Retry-After`
/// (secondary limits), else the reset time of an exhausted primary limit.
fn rate_limit_wait(status: StatusCode, headers: &HeaderMap, now: u64) -> Option<u64> {
    let number = |name: &str| headers.get(name)?.to_str().ok()?.trim().parse::<u64>().ok();
    if let Some(secs) = number("retry-after") {
        return Some(secs.max(1));
    }
    if number("x-ratelimit-remaining") == Some(0)
        && let Some(reset) = number("x-ratelimit-reset")
    {
        return Some(reset.saturating_sub(now).max(1));
    }
    (status == StatusCode::TOO_MANY_REQUESTS).then_some(DEFAULT_RATE_LIMIT_SECS)
}

/// GitHub's `message` from a JSON error body; otherwise a short excerpt of the body, or the
/// status text when the body is empty.
fn error_message(body: &str, status: StatusCode) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    if let Some(message) = parsed.as_ref().and_then(|json| json["message"].as_str()) {
        return message.to_owned();
    }
    let text = body.trim();
    if text.is_empty() {
        return status.canonical_reason().unwrap_or("no details").to_owned();
    }
    let mut excerpt: String = text.chars().take(MAX_ERROR_TEXT).collect();
    if excerpt.len() < text.len() {
        excerpt.push('…');
    }
    excerpt
}

fn extract_data(mut response: Value, strict: bool) -> Result<Value> {
    let data = response["data"].take();
    let messages: Vec<&str> = response["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|error| error["message"].as_str())
        .collect();
    if !messages.is_empty() && (strict || data.is_null()) {
        return Err(Error::GraphQl(messages.join("; ")));
    }
    if data.is_null() {
        return Err(Error::Parse("GraphQL response without data".to_owned()));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn data_and_errors() {
        let ok = json!({"data": {"a": 1}});
        assert_eq!(extract_data(ok, true).unwrap(), json!({"a": 1}));
        let partial = json!({"data": {"a": 1}, "errors": [{"message": "x"}]});
        assert!(extract_data(partial.clone(), false).is_ok());
        assert!(matches!(
            extract_data(partial, true),
            Err(Error::GraphQl(_))
        ));
        let failed = json!({"data": null, "errors": [{"message": "boom"}]});
        assert!(matches!(extract_data(failed, false), Err(Error::GraphQl(m)) if m == "boom"));
    }

    #[test]
    fn error_messages_are_short() {
        let status = StatusCode::UNPROCESSABLE_ENTITY;
        let json = r#"{"message":"Validation Failed","documentation_url":"https://docs"}"#;
        assert_eq!(error_message(json, status), "Validation Failed");
        assert_eq!(error_message("", status), "Unprocessable Entity");
        let html = format!("<html>{}</html>", "x".repeat(500));
        let excerpt = error_message(&html, StatusCode::BAD_GATEWAY);
        assert_eq!(excerpt.chars().count(), MAX_ERROR_TEXT + 1);
        assert!(excerpt.ends_with('…'));
    }

    #[test]
    fn rate_limits_are_recognized() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.insert(*name, HeaderValue::from_static(value));
            }
            map
        };
        let forbidden = StatusCode::FORBIDDEN;
        let retry = headers(&[("retry-after", "120")]);
        assert_eq!(rate_limit_wait(forbidden, &retry, 0), Some(120));
        let primary = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1300"),
        ]);
        assert_eq!(rate_limit_wait(forbidden, &primary, 1000), Some(300));
        assert_eq!(rate_limit_wait(forbidden, &primary, 2000), Some(1));
        let left = headers(&[
            ("x-ratelimit-remaining", "10"),
            ("x-ratelimit-reset", "1300"),
        ]);
        assert_eq!(rate_limit_wait(forbidden, &left, 1000), None);
        assert_eq!(
            rate_limit_wait(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), 0),
            Some(DEFAULT_RATE_LIMIT_SECS)
        );
    }
}
