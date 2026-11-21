//! Every event the backend sends to the UI goes through here, with its typed payload. The
//! event names are constants in `protocol`, shared with the UI.
//!
//! A UI that misses an event only shows stale data until the next one; the backend never
//! depends on delivery, so emit errors are ignored.

use crate::exec::BatchEvent;
use crate::state::{AppState, lock};
use protocol::{
    ActionProgress, EVENT_ACTION_PROGRESS, EVENT_INBOX_UPDATED, EVENT_REFRESHING, InboxSnapshot,
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

/// `inbox-updated` with `snapshot`.
pub(crate) fn inbox_updated(app: &AppHandle, snapshot: &InboxSnapshot) {
    let _ = app.emit(EVENT_INBOX_UPDATED, snapshot);
}

/// `inbox-updated` with the cached snapshot (after a local change), if there is one.
pub(crate) fn inbox_cached(app: &AppHandle) {
    let snapshot = lock(&app.state::<AppState>().snapshot).clone();
    if let Some(snapshot) = snapshot {
        inbox_updated(app, &snapshot);
    }
}

/// Sends the events of a batch of actions: progress as `action-progress`, a changed snapshot as
/// `inbox-updated`.
pub(crate) fn batch(app: &AppHandle) -> impl Fn(BatchEvent) + Sync + use<'_> {
    |event| match event {
        BatchEvent::Progress(progress) => action_progress(app, &progress),
        BatchEvent::SnapshotChanged => inbox_cached(app),
    }
}

fn action_progress(app: &AppHandle, progress: &ActionProgress) {
    let _ = app.emit(EVENT_ACTION_PROGRESS, progress);
}

/// Marks a background refresh as running until dropped. `refreshing` is `true` while at least
/// one runs, so overlapping refreshes do not end the spinner early.
pub(crate) struct Refreshing<'a> {
    app: &'a AppHandle,
}

pub(crate) fn refreshing(app: &AppHandle) -> Refreshing<'_> {
    let running = &app.state::<AppState>().background_refreshes;
    if running.fetch_add(1, Ordering::SeqCst) == 0 {
        let _ = app.emit(EVENT_REFRESHING, true);
    }
    Refreshing { app }
}

impl Drop for Refreshing<'_> {
    fn drop(&mut self) {
        let running = &self.app.state::<AppState>().background_refreshes;
        if running.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = self.app.emit(EVENT_REFRESHING, false);
        }
    }
}

/// `show-about`: the macOS app menu asks for the About tab.
#[cfg(target_os = "macos")]
pub(crate) fn show_about<R: tauri::Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(protocol::EVENT_SHOW_ABOUT, ());
}
