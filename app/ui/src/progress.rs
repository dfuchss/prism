//! The progress panel under the top bar: the running (or last) batch of actions.

use crate::format::progress_verb;
use crate::state::Ui;
use crate::widgets::result_line;
use leptos::prelude::*;
use protocol::{Action, ActionProgress, ActionResult, PrRef, ProgressSource};
use std::collections::BTreeMap;

/// State of a PR in the last batch, shown in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Running,
    Done,
    Failed,
}

/// How an `action-progress` event relates to the batch shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fit {
    /// Of an older batch: ignored.
    Stale,
    /// Of the batch shown.
    Same,
    /// Of a newer batch (or there is none): it replaces the one shown.
    New,
}

impl Fit {
    pub(crate) fn of(shown: Option<u64>, event: u64) -> Self {
        match shown {
            Some(id) if event < id => Self::Stale,
            Some(id) if event == id => Self::Same,
            _ => Self::New,
        }
    }
}

/// A batch of actions as the events described it so far.
#[derive(Debug, Clone)]
pub(crate) struct Batch {
    pub id: u64,
    pub total: usize,
    pub done: usize,
    /// The PR being worked on right now.
    pub working: Option<PrRef>,
    pub action: Action,
    pub source: ProgressSource,
    pub lines: Vec<ActionResult>,
    pub finished: bool,
}

impl Batch {
    pub(crate) fn new(progress: &ActionProgress) -> Self {
        Self {
            id: progress.batch_id,
            total: progress.total,
            done: 0,
            working: None,
            action: progress.action.clone(),
            source: progress.source,
            lines: Vec::new(),
            finished: false,
        }
    }

    pub(crate) fn apply(&mut self, progress: ActionProgress) {
        self.done = progress.done;
        self.working = progress.current.filter(|_| progress.last.is_none());
        self.action = progress.action;
        self.lines.extend(progress.last);
        self.finished = self.done >= self.total;
    }

    pub(crate) fn running(&self) -> bool {
        !self.finished
    }

    pub(crate) fn all_ok(&self) -> bool {
        self.lines.iter().all(|line| line.ok)
    }

    pub(crate) fn title(&self) -> String {
        let verb = progress_verb(&self.action, self.source);
        if self.finished {
            let failed = self.lines.iter().filter(|line| !line.ok).count();
            let ok = self.lines.len() - failed;
            return format!("{verb}: finished, {ok} ok, {failed} failed");
        }
        let at = (self.done + usize::from(self.working.is_some())).min(self.total);
        match &self.working {
            Some(pr) => format!("{verb} {at}/{} \u{2014} {pr}", self.total),
            None => format!("{verb} {at}/{}", self.total),
        }
    }

    pub(crate) fn percent(&self) -> usize {
        self.done * 100 / self.total.max(1)
    }
}

/// Marks the PR of an event: running while its job runs, then ok or failed (failed sticks).
pub(crate) fn update_marks(marks: &mut BTreeMap<PrRef, Mark>, progress: &ActionProgress) {
    if let Some(last) = &progress.last {
        let failed = !last.ok || marks.get(&last.pr) == Some(&Mark::Failed);
        marks.insert(
            last.pr.clone(),
            if failed { Mark::Failed } else { Mark::Done },
        );
    } else if let Some(pr) = &progress.current {
        marks.insert(pr.clone(), Mark::Running);
    }
}

#[component]
pub(crate) fn Progress() -> impl IntoView {
    let ui = expect_context::<Ui>();
    let expanded = RwSignal::new(true);
    let running = Memo::new(move |_| ui.batch.with(|b| b.as_ref().is_some_and(Batch::running)));
    let finished = Memo::new(move |_| ui.batch.with(|b| b.as_ref().is_some_and(|b| b.finished)));
    let failed = Memo::new(move |_| ui.batch.with(|b| b.as_ref().is_some_and(|b| !b.all_ok())));
    view! {
        <Show when=move || ui.batch.with(Option::is_some)>
            <section class="progress">
                <div class="progress-head">
                    <span class="spin reserve" class:on=running></span>
                    <strong>{move || ui.batch.with(|b| b.as_ref().map(Batch::title))}</strong>
                    <span class="spacer"></span>
                    <button class="ghost" on:click=move |_| expanded.update(|e| *e = !*e)>
                        {move || if expanded.get() { "Hide details" } else { "Show details" }}
                    </button>
                    <Show when=move || finished.get()>
                        <button class="ghost" on:click=move |_| ui.batch.set(None)>"Dismiss"</button>
                    </Show>
                </div>
                <div class="bar">
                    <div
                        class="fill"
                        class:bad=failed
                        style:width=move || {
                            format!("{}%", ui.batch.with(|b| b.as_ref().map_or(0, Batch::percent)))
                        }
                    ></div>
                </div>
                <Show when=move || expanded.get()>
                    <div class="progress-list">
                        {move || {
                            ui.batch.with(|b| {
                                b.as_ref().map(|b| b.lines.iter().map(result_line).collect_view())
                            })
                        }}
                    </div>
                </Show>
            </section>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(number: u64) -> PrRef {
        PrRef {
            repo: "a/b".to_owned(),
            number,
        }
    }

    fn event(done: usize, current: Option<u64>, last: Option<(u64, bool)>) -> ActionProgress {
        ActionProgress {
            batch_id: 3,
            total: 2,
            done,
            current: current.map(pr),
            action: Action::MarkDone,
            last: last.map(|(number, ok)| ActionResult {
                pr: pr(number),
                action: Action::MarkDone,
                ok,
                message: String::new(),
            }),
            source: ProgressSource::Manual,
        }
    }

    #[test]
    fn fit_compares_batch_ids() {
        assert_eq!(Fit::of(None, 1), Fit::New);
        assert_eq!(Fit::of(Some(2), 1), Fit::Stale);
        assert_eq!(Fit::of(Some(2), 2), Fit::Same);
        assert_eq!(Fit::of(Some(2), 3), Fit::New);
    }

    #[test]
    fn batch_follows_start_job_and_result_events() {
        let start = event(0, None, None);
        let mut batch = Batch::new(&start);
        batch.apply(start);
        assert!(batch.running() && batch.working.is_none());
        assert_eq!(batch.title(), "Marking done 0/2");
        batch.apply(event(0, Some(1), None));
        assert_eq!(batch.working, Some(pr(1)));
        assert_eq!(batch.title(), "Marking done 1/2 \u{2014} a/b#1");
        batch.apply(event(1, Some(1), Some((1, true))));
        assert!(batch.working.is_none() && batch.running());
        assert_eq!(batch.percent(), 50);
        batch.apply(event(2, Some(2), Some((2, false))));
        assert!(batch.finished && !batch.all_ok());
        assert_eq!(batch.lines.len(), 2);
        assert_eq!(batch.title(), "Marking done: finished, 1 ok, 1 failed");
    }

    #[test]
    fn marks_run_then_settle_and_failures_stick() {
        let mut marks = BTreeMap::new();
        update_marks(&mut marks, &event(0, None, None));
        assert!(marks.is_empty());
        update_marks(&mut marks, &event(0, Some(1), None));
        assert_eq!(marks.get(&pr(1)), Some(&Mark::Running));
        update_marks(&mut marks, &event(1, Some(1), Some((1, false))));
        assert_eq!(marks.get(&pr(1)), Some(&Mark::Failed));
        // A later success of the same PR (approve + merge) does not hide the failure.
        update_marks(&mut marks, &event(1, Some(1), Some((1, true))));
        assert_eq!(marks.get(&pr(1)), Some(&Mark::Failed));
        update_marks(&mut marks, &event(2, Some(2), Some((2, true))));
        assert_eq!(marks.get(&pr(2)), Some(&Mark::Done));
    }
}
