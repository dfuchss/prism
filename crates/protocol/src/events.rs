//! Names of the events the backend sends to the UI, with the type of their payload.

/// Payload [`InboxSnapshot`](crate::InboxSnapshot): the cached inbox changed (a fetch, a local
/// update after an action, a rescope or a rules change).
pub const EVENT_INBOX_UPDATED: &str = "inbox-updated";
/// Payload [`ActionProgress`](crate::ActionProgress): a batch of actions started, a job started
/// or a result came in.
pub const EVENT_ACTION_PROGRESS: &str = "action-progress";
/// Payload `bool`: whether a background refresh (after a settings change) runs.
pub const EVENT_REFRESHING: &str = "refreshing";
/// Payload `()`: the macOS app menu asks for the About tab.
pub const EVENT_SHOW_ABOUT: &str = "show-about";
