//! Types shared by the Prism UI and backend. Everything here is plain data (serde) plus small
//! pure helpers; no I/O. The rules-file schema lives in `prism-rules`, the parsing of GitHub
//! data (including bump titles and duplicates) in `prism-github`.

mod about;
mod action;
mod bucket;
mod bump;
mod capabilities;
mod events;
mod inbox;
mod time;

pub use about::{AppInfo, ScopePreview, Viewer};
pub use action::{
    ActTarget, Action, ActionProgress, ActionResult, AuditEntry, FailedJob, PlannedAction,
    ProgressSource,
};
pub use bucket::bucket;
pub use bump::{Bump, UpdateKind};
pub use capabilities::{AutoMerge, Capabilities, unavailable_reason};
pub use events::{EVENT_ACTION_PROGRESS, EVENT_INBOX_UPDATED, EVENT_REFRESHING, EVENT_SHOW_ABOUT};
pub use inbox::{
    Bucket, CheckRun, CheckState, CiSummary, InboxSnapshot, MergeMethod, MergeState,
    NotificationInfo, NotifyReason, PrItem, PrRef, PrState, ReviewDecision,
};
pub use time::{now_rfc3339, rfc3339_secs_ago};
