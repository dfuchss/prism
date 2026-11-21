//! UI state shared by all components (provided as context).

use crate::actions::{self, CommentBox};
use crate::api;
use crate::filter::{Filter, View, Who, visible};
use crate::format::{action_label, count};
use crate::progress::{Batch, Fit, Mark};
use leptos::prelude::*;
use protocol::{Action, ActionProgress, ActionResult, InboxSnapshot, PlannedAction, PrItem, PrRef};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Inbox,
    Planned,
    Rules,
    Settings,
    Audit,
    About,
}

impl Tab {
    /// The tabs on the left; `About` sits on the right of the top bar.
    pub(crate) const MAIN: [Self; 5] = [
        Self::Inbox,
        Self::Planned,
        Self::Rules,
        Self::Settings,
        Self::Audit,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Planned => "Planned",
            Self::Rules => "Rules",
            Self::Settings => "Settings",
            Self::Audit => "Audit",
            Self::About => "About",
        }
    }
}

/// What the user started and still waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity {
    Idle,
    /// The Refresh button's refresh.
    Refreshing,
    /// Manual or planned actions.
    Acting,
}

/// What a confirm dialog will run once accepted.
#[derive(Debug, Clone)]
pub(crate) enum Task {
    Act {
        prs: Vec<PrRef>,
        /// Head commits shown when the dialog opened; the backend refuses PRs that moved on.
        heads: Vec<String>,
        action: Action,
    },
    Planned(Vec<PlannedAction>),
}

#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub title: String,
    pub lines: Vec<String>,
    pub task: Task,
    pub comment: CommentBox,
}

/// The text of a TOML editor tab; kept here so it survives switching tabs.
#[derive(Clone, Copy)]
pub(crate) struct Editor {
    pub text: RwSignal<String>,
    /// The text as last loaded or saved; `None` until loaded.
    pub saved: RwSignal<Option<String>>,
    pub saving: RwSignal<bool>,
    pub outcome: RwSignal<Option<Result<Vec<String>, String>>>,
    /// Changed since loaded or saved (typed into while the file could not be loaded counts).
    pub dirty: Memo<bool>,
}

impl Editor {
    fn new() -> Self {
        let text = RwSignal::new(String::new());
        let saved = RwSignal::new(None::<String>);
        Self {
            text,
            saved,
            saving: RwSignal::new(false),
            outcome: RwSignal::new(None),
            dirty: Memo::new(move |_| {
                saved.with(|saved| {
                    text.with(|text| saved.as_ref().map_or(!text.is_empty(), |s| text != s))
                })
            }),
        }
    }
}

/// The current time in ms, ticking once a second (for ages).
#[derive(Clone, Copy)]
pub(crate) struct Clock(pub RwSignal<f64>);

impl Clock {
    pub(crate) fn new() -> Self {
        Self(RwSignal::new(crate::format::now_ms()))
    }
}

/// The TOML editor tabs (a context of their own, to keep [`Ui`] small to copy).
#[derive(Clone, Copy)]
pub(crate) struct Editors {
    pub rules: Editor,
    pub settings: Editor,
}

impl Editors {
    pub(crate) fn new() -> Self {
        Self {
            rules: Editor::new(),
            settings: Editor::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Ui {
    /// Only set through [`Ui::set_snapshot`].
    pub snapshot: RwSignal<Option<InboxSnapshot>>,
    /// Position of each PR in `snapshot.items`.
    index: Memo<HashMap<PrRef, usize>>,
    pub tab: RwSignal<Tab>,
    pub filter: RwSignal<Filter>,
    /// Selected PRs, including ones the filter hides (they are kept, but never acted on).
    pub selected: RwSignal<BTreeSet<PrRef>>,
    pub open: RwSignal<Option<PrRef>>,
    /// The items the table shows, most recently updated first.
    pub shown: Memo<Vec<PrItem>>,
    /// The selected PRs the table shows: what bulk actions act on.
    pub chosen: Memo<Vec<PrRef>>,
    pub results: RwSignal<Vec<ActionResult>>,
    pub notice: RwSignal<Option<String>>,
    pub pending: RwSignal<Option<Pending>>,
    pub activity: RwSignal<Activity>,
    /// Whether actions must not be started: actions run or a batch is shown running.
    pub locked: Memo<bool>,
    /// A background refresh (after a settings change) runs.
    pub refreshing: RwSignal<bool>,
    /// The batch of actions that runs (or ran last), until dismissed.
    pub batch: RwSignal<Option<Batch>>,
    /// Per-PR state of the last batch, shown in the table until the next refresh.
    pub marks: RwSignal<BTreeMap<PrRef, Mark>>,
}

impl Ui {
    pub(crate) fn new() -> Self {
        let snapshot = RwSignal::new(None::<InboxSnapshot>);
        let filter = RwSignal::new(Filter::default());
        let selected = RwSignal::new(BTreeSet::new());
        let activity = RwSignal::new(Activity::Idle);
        let batch = RwSignal::new(None::<Batch>);
        let shown = Memo::new(move |_| {
            filter.with(|filter| {
                snapshot.with(|s| s.as_ref().map_or_default(|s| visible(&s.items, filter)))
            })
        });
        Self {
            snapshot,
            index: Memo::new(move |_| {
                snapshot.with(|s| {
                    s.as_ref().map_or_default(|s| {
                        s.items
                            .iter()
                            .enumerate()
                            .map(|(at, item)| (item.pr.clone(), at))
                            .collect()
                    })
                })
            }),
            tab: RwSignal::new(Tab::Inbox),
            filter,
            selected,
            open: RwSignal::new(None),
            shown,
            chosen: Memo::new(move |_| {
                selected.with(|selected| {
                    shown.with(|shown| {
                        shown
                            .iter()
                            .filter(|item| selected.contains(&item.pr))
                            .map(|item| item.pr.clone())
                            .collect()
                    })
                })
            }),
            results: RwSignal::new(Vec::new()),
            notice: RwSignal::new(None),
            pending: RwSignal::new(None),
            activity,
            locked: Memo::new(move |_| {
                activity.get() == Activity::Acting
                    || batch.with(|batch| batch.as_ref().is_some_and(Batch::running))
            }),
            refreshing: RwSignal::new(false),
            batch,
            marks: RwSignal::new(BTreeMap::new()),
        }
    }

    /// Shows a new snapshot. Ignores one older than the shown one (a command's answer can
    /// arrive after the event of a later refresh) and an unchanged one. Forgets the selection,
    /// open PR and repository filter of PRs that are gone, and the row marks unless a batch
    /// still runs.
    pub(crate) fn set_snapshot(&self, snapshot: InboxSnapshot) {
        let skip = self.snapshot.with_untracked(|current| {
            current
                .as_ref()
                .is_some_and(|c| snapshot.fetched_at < c.fetched_at || *c == snapshot)
        });
        if skip {
            return;
        }
        let present: BTreeSet<&PrRef> = snapshot.items.iter().map(|item| &item.pr).collect();
        if self
            .selected
            .with_untracked(|set| set.iter().any(|pr| !present.contains(pr)))
        {
            self.selected
                .update(|set| set.retain(|pr| present.contains(pr)));
        }
        if self
            .open
            .with_untracked(|open| open.as_ref().is_some_and(|pr| !present.contains(pr)))
        {
            self.open.set(None);
        }
        let repo = self.filter.with_untracked(|filter| filter.repo.clone());
        let pruned = repo.clone().pruned(&snapshot.items);
        if pruned != repo {
            self.filter.update(|filter| filter.repo = pruned);
        }
        if !self
            .batch
            .with_untracked(|batch| batch.as_ref().is_some_and(Batch::running))
        {
            self.marks.set(BTreeMap::new());
        }
        self.snapshot.set(Some(snapshot));
    }

    /// Folds an `action-progress` event into the batch and the row marks.
    pub(crate) fn on_progress(&self, progress: ActionProgress) {
        let fit = self
            .batch
            .with_untracked(|batch| Fit::of(batch.as_ref().map(|b| b.id), progress.batch_id));
        if fit == Fit::Stale {
            return;
        }
        if fit == Fit::New {
            self.marks.set(BTreeMap::new());
        }
        self.marks
            .update(|marks| crate::progress::update_marks(marks, &progress));
        let mut hide = None;
        self.batch.update(|slot| {
            let batch = match (fit, slot) {
                (Fit::Same, Some(batch)) => batch,
                (_, slot) => slot.insert(Batch::new(&progress)),
            };
            batch.apply(progress);
            if batch.finished && batch.all_ok() {
                hide = Some(batch.id);
            }
        });
        if let Some(id) = hide {
            self.hide_later(id);
        }
    }

    /// Hides a successful batch after a few seconds.
    fn hide_later(&self, id: u64) {
        let ui = *self;
        set_timeout(
            move || {
                ui.batch.update(|batch| {
                    if batch.as_ref().is_some_and(|b| b.id == id && b.finished) {
                        *batch = None;
                    }
                });
            },
            std::time::Duration::from_secs(5),
        );
    }

    /// Runs `f` on the item of `pr` in the snapshot (tracks the snapshot).
    pub(crate) fn with_item<R>(&self, pr: &PrRef, f: impl FnOnce(Option<&PrItem>) -> R) -> R {
        let at = self.index.with(|index| index.get(pr).copied());
        self.snapshot.with(|snapshot| {
            f(snapshot
                .as_ref()
                .and_then(|s| at.and_then(|at| s.items.get(at)))
                .filter(|item| item.pr == *pr))
        })
    }

    /// Runs `f` on the items of `prs` that are in the snapshot (tracks the snapshot).
    pub(crate) fn with_items<R>(&self, prs: &[PrRef], f: impl FnOnce(Vec<&PrItem>) -> R) -> R {
        self.index.with(|index| {
            self.snapshot.with(|snapshot| {
                let items = snapshot.as_ref().map_or_default(|s| {
                    prs.iter()
                        .filter_map(|pr| {
                            index
                                .get(pr)
                                .and_then(|&at| s.items.get(at))
                                .filter(|item| item.pr == *pr)
                        })
                        .collect()
                });
                f(items)
            })
        })
    }

    /// The item of `pr`, changing only when the item does.
    pub(crate) fn item(&self, pr: PrRef) -> Memo<Option<PrItem>> {
        let ui = *self;
        Memo::new(move |_| ui.with_item(&pr, |item: Option<&PrItem>| item.cloned()))
    }

    pub(crate) fn set_view(&self, view: View) {
        self.filter.update(|filter| filter.view = view);
    }

    pub(crate) fn toggle_who(&self, who: Who) {
        self.filter.update(|filter| {
            filter.who = if filter.who == who {
                Who::Everyone
            } else {
                who
            };
        });
    }

    pub(crate) fn toggle_selected(&self, pr: &PrRef) {
        self.selected.update(|set| {
            if !set.remove(pr) {
                set.insert(pr.clone());
            }
        });
    }

    /// `owner/repo#1 - title` of a PR in the inbox, for dialogs.
    fn describe(&self, pr: &PrRef) -> String {
        self.with_item_untracked(pr, |item| match item {
            Some(item) => format!("{pr} - {}", item.title),
            None => pr.to_string(),
        })
    }

    fn with_item_untracked<R>(&self, pr: &PrRef, f: impl FnOnce(Option<&PrItem>) -> R) -> R {
        untrack(|| self.with_item(pr, f))
    }

    /// The head commits of the PRs as currently shown (empty when unknown).
    fn heads(&self, prs: &[PrRef]) -> Vec<String> {
        prs.iter()
            .map(|pr| {
                self.with_item_untracked(pr, |item| item.map_or_default(|i| i.head_oid.clone()))
            })
            .collect()
    }

    /// Runs `action` on the PRs, bound to the head commits the user saw.
    pub(crate) fn run_seen(&self, prs: Vec<PrRef>, heads: Vec<String>, action: Action) {
        self.start(async move { api::act(&prs, &heads, &action).await });
    }

    pub(crate) fn run_planned(&self, planned: Vec<PlannedAction>) {
        self.start(async move { api::apply_planned(&planned).await });
    }

    fn start(
        &self,
        work: impl std::future::Future<Output = Result<Vec<ActionResult>, String>> + 'static,
    ) {
        let ui = *self;
        ui.activity.set(Activity::Acting);
        leptos::task::spawn_local(async move {
            match work.await {
                Ok(results) => ui.finished(results),
                Err(error) => ui.notice.set(Some(error)),
            }
            // The events may be lost (or late); the command returning means the batch is over.
            ui.batch.update(|batch| {
                if let Some(batch) = batch {
                    batch.finished = true;
                }
            });
            ui.activity.set(Activity::Idle);
        });
    }

    /// Shows the results and unselects the PRs whose action worked.
    fn finished(&self, results: Vec<ActionResult>) {
        self.selected.update(|selected| {
            for result in results.iter().filter(|result| result.ok) {
                selected.remove(&result.pr);
            }
        });
        // The progress panel lists the results; without events (none came in) fall back to the toast.
        if self.batch.with_untracked(Option::is_none) {
            self.results.set(results);
        }
    }

    /// Splits `prs` into those `action` works on and the `skipped: N - reason` lines of the
    /// others. PRs not in the inbox count as supported.
    fn split_supported(&self, prs: Vec<PrRef>, action: &Action) -> (Vec<PrRef>, Vec<String>) {
        let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
        let supported = prs
            .into_iter()
            .filter(|pr| {
                let reason = self
                    .with_item_untracked(pr, |item| item.and_then(|i| actions::reason(i, action)));
                reason
                    .map(|r| *skipped.entry(r).or_default() += 1)
                    .is_none()
            })
            .collect();
        (supported, actions::skipped_lines(skipped))
    }

    /// Runs `action` right away, or asks first when some of the PRs have to be skipped.
    pub(crate) fn run_or_confirm(&self, prs: Vec<PrRef>, action: Action) {
        let (supported, skipped) = self.split_supported(prs, &action);
        if skipped.is_empty() {
            let heads = self.heads(&supported);
            self.run_seen(supported, heads, action);
        } else {
            self.ask(supported, skipped, action, CommentBox::None);
        }
    }

    /// Asks for confirmation before running `action` on `prs`; PRs the action is unavailable
    /// for are skipped and listed.
    pub(crate) fn confirm(&self, prs: Vec<PrRef>, action: Action, comment: CommentBox) {
        let (prs, skipped) = self.split_supported(prs, &action);
        self.ask(prs, skipped, action, comment);
    }

    fn ask(&self, prs: Vec<PrRef>, skipped: Vec<String>, action: Action, comment: CommentBox) {
        if prs.is_empty() {
            return;
        }
        let verb = action_label(&action);
        let mut verb_chars = verb.chars();
        let verb = verb_chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(verb_chars).collect::<String>()
        });
        let title = format!("{verb} {}?", count(prs.len(), "pull request"));
        let lines = prs
            .iter()
            .map(|pr| self.describe(pr))
            .chain(skipped)
            .collect();
        let heads = self.heads(&prs);
        self.pending.set(Some(Pending {
            title,
            lines,
            task: Task::Act { prs, heads, action },
            comment,
        }));
    }

    pub(crate) fn confirm_planned(&self, planned: Vec<PlannedAction>) {
        let lines = planned
            .iter()
            .map(|p| {
                format!(
                    "{}: {} {}",
                    p.rule,
                    action_label(&p.action),
                    self.describe(&p.pr)
                )
            })
            .collect();
        self.pending.set(Some(Pending {
            title: format!("Apply {}?", count(planned.len(), "planned action")),
            lines,
            task: Task::Planned(planned),
            comment: CommentBox::None,
        }));
    }
}
