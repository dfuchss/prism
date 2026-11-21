//! Runs actions: manual ones, applied suggestions and `auto` rules, always auditing.

use crate::config;
use crate::state::{AppState, lock};
use prism_rules::{Rule, auto_allowed, matches};
use protocol::{
    ActTarget, Action, ActionProgress, ActionResult, AuditEntry, PlannedAction, PrItem, PrRef,
    PrState, ProgressSource, now_rfc3339, unavailable_reason,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// How many keys each part of the [`AutoMemo`] keeps.
const MEMO_LIMIT: usize = 2000;

/// A set of keys in insertion order that forgets its oldest keys beyond [`MEMO_LIMIT`].
#[derive(Default)]
pub(crate) struct BoundedSet {
    order: VecDeque<String>,
    members: HashSet<String>,
}

impl BoundedSet {
    fn contains(&self, key: &str) -> bool {
        self.members.contains(key)
    }

    /// Adds `key`; `false` when it was there already.
    fn insert(&mut self, key: String) -> bool {
        if self.members.contains(&key) {
            return false;
        }
        self.members.insert(key.clone());
        self.order.push_back(key);
        while self.order.len() > MEMO_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.members.remove(&oldest);
            }
        }
        true
    }
}

/// What `auto` rules did already, so a poll does not repeat an action (a second approval, a
/// repeated comment). `executed` is persisted in the config directory (`auto-memo.json`), so a
/// restart does not repeat it either; failures and skip messages are remembered for this
/// session only.
#[derive(Default)]
pub(crate) struct AutoMemo {
    executed: BoundedSet,
    failed: BoundedSet,
    skipped: BoundedSet,
}

impl AutoMemo {
    /// The memo with the persisted keys of executed actions (oldest first).
    pub(crate) fn new(executed: Vec<String>) -> Self {
        let mut memo = Self::default();
        for key in executed {
            memo.executed.insert(key);
        }
        memo
    }

    /// The keys of the executed actions as the JSON array to persist.
    fn executed_json(&self) -> Option<String> {
        serde_json::to_string(&self.executed.order).ok()
    }
}

/// The rule behind a job: its position in the rule list the plan was made from, and its name.
pub(crate) struct RuleRef {
    pub index: usize,
    pub name: String,
}

/// One action to run, with the rule that asked for it (`None`: the user).
pub(crate) struct Job {
    pub rule: Option<RuleRef>,
    pub pr: PrRef,
    pub action: Action,
    /// The head commit the user (or the plan) saw; `None`: unknown, the cached one counts.
    pub seen_head: Option<String>,
}

impl Job {
    pub(crate) fn manual(target: ActTarget, action: Action) -> Self {
        Self {
            rule: None,
            pr: target.pr,
            action,
            seen_head: target.head.filter(|head| !head.is_empty()),
        }
    }

    pub(crate) fn planned(planned: PlannedAction) -> Self {
        Self {
            rule: Some(RuleRef {
                index: planned.rule_index,
                name: planned.rule,
            }),
            pr: planned.pr,
            action: planned.action,
            seen_head: Some(planned.head_oid).filter(|head| !head.is_empty()),
        }
    }
}

/// Approving, merging and rebasing actions act on the current head commit and checks, so they use fresh
/// details.
fn needs_fresh(action: &Action) -> bool {
    matches!(
        action,
        Action::Approve
            | Action::Merge(_)
            | Action::ApproveAndMerge(_)
            | Action::EnableAutoMerge(_)
            | Action::Rebase
    )
}

/// Whether a job runs against fresh details: head-bound actions always, and in `auto` passes
/// everything except notification-only actions (they need neither fresh checks nor a head).
fn wants_fresh(action: &Action, auto: bool) -> bool {
    needs_fresh(action) || (auto && !action.is_notification_only())
}

type Items = HashMap<PrRef, PrItem>;

/// What the jobs of one run are checked and executed against.
struct Context<'a> {
    cached: Items,
    fresh: Result<Items, String>,
    /// `Some` for `auto` passes: every job must pass the safety gate.
    auto: Option<&'a [Rule]>,
    /// The current rules, to re-check applied suggestions.
    rules: Vec<Rule>,
}

impl Context<'_> {
    fn uses_fresh(&self, job: &Job) -> bool {
        wants_fresh(&job.action, self.auto.is_some())
    }

    fn item(&self, job: &Job) -> Result<&PrItem, String> {
        let source = if self.uses_fresh(job) {
            self.fresh.as_ref().map_err(Clone::clone)?
        } else {
            &self.cached
        };
        source.get(&job.pr).ok_or_else(|| {
            if self.uses_fresh(job) {
                "no longer open".to_owned()
            } else {
                "not in the inbox".to_owned()
            }
        })
    }

    /// The safety gate. `auto` jobs: the rule still matches the fresh item and allows the
    /// action. Other head-bound jobs: the head is still the one the user or plan saw, and an
    /// applied suggestion's rule still matches the fresh item.
    fn gate(&self, job: &Job, item: &PrItem) -> Result<(), String> {
        if item.state != PrState::Open && !job.action.is_notification_only() {
            return Err("skipped: the pull request is merged or closed".to_owned());
        }
        if let Some(reason) = unavailable_reason(item, &job.action) {
            return Err(format!("skipped: {reason}"));
        }
        let Some(rules) = self.auto else {
            if !self.uses_fresh(job) {
                return Ok(());
            }
            self.same_head(job, item)?;
            return match &job.rule {
                Some(rule) => rule_matches(&self.rules, rule, item).map(drop),
                None => Ok(()),
            };
        };
        let rule = job
            .rule
            .as_ref()
            .ok_or_else(|| "skipped: auto jobs need a rule".to_owned())?;
        let rule = rule_matches(rules, rule, item)?;
        if !auto_allowed(rule, item, &job.action) {
            return Err(
                "skipped: auto rules only act on involved PRs and merge only green patch/minor updates (checked just now)"
                    .to_owned(),
            );
        }
        Ok(())
    }

    /// The fresh head must be the one that was seen (or, unknown, the cached one).
    fn same_head(&self, job: &Job, fresh: &PrItem) -> Result<(), String> {
        let seen = job
            .seen_head
            .as_deref()
            .or_else(|| self.cached.get(&job.pr).map(|item| item.head_oid.as_str()))
            .filter(|head| !head.is_empty())
            .ok_or_else(|| "skipped: not in the inbox (refresh and retry)".to_owned())?;
        if seen == fresh.head_oid {
            Ok(())
        } else {
            Err("skipped: new commits since you looked (refresh and retry)".to_owned())
        }
    }
}

/// The rule `wanted` refers to (same position, same name), if it exists and still matches
/// `item`.
fn rule_matches<'r>(
    rules: &'r [Rule],
    wanted: &RuleRef,
    item: &PrItem,
) -> Result<&'r Rule, String> {
    let rule = rules
        .get(wanted.index)
        .filter(|rule| rule.name == wanted.name)
        .ok_or_else(|| "skipped: rule no longer exists (rules changed)".to_owned())?;
    if !matches(rule, item) {
        return Err("skipped: rule no longer matches the current state".to_owned());
    }
    Ok(rule)
}

/// What a run reports while it runs.
pub(crate) enum BatchEvent {
    Progress(ActionProgress),
    /// The cached snapshot changed (throttled, see [`Throttle`]).
    SnapshotChanged,
}

/// Receives the events of a run (the Tauri event emitter in the app).
pub(crate) type Emit<'a> = &'a (dyn Fn(BatchEvent) + Sync);

/// Minimum time between two snapshot announcements during a batch.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(250);

/// Throttles snapshot announcements: the first change is announced at once, later ones at most
/// once per interval; a change held back is announced at the next check after the interval (a
/// job boundary) or by [`Throttle::finish`].
struct Throttle {
    interval: Duration,
    last: Option<Instant>,
    pending: bool,
}

impl Throttle {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
            pending: false,
        }
    }

    /// Notes whether the snapshot `changed`; whether to announce it now.
    fn should_emit(&mut self, changed: bool, now: Instant) -> bool {
        self.pending |= changed;
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if self.pending && due {
            self.pending = false;
            self.last = Some(now);
            return true;
        }
        false
    }

    /// At the end of the batch: whether a change is still unannounced.
    fn finish(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

/// Counts the jobs of one batch and reports them.
struct Batch<'a> {
    id: u64,
    total: usize,
    source: ProgressSource,
    emit: Emit<'a>,
}

impl Batch<'_> {
    fn send(&self, done: usize, job: &Job, current: bool, last: Option<ActionResult>) {
        (self.emit)(BatchEvent::Progress(ActionProgress {
            batch_id: self.id,
            total: self.total,
            done,
            current: current.then(|| job.pr.clone()),
            action: job.action.clone(),
            last,
            source: self.source,
        }));
    }

    /// Announces the snapshot when the throttle says so.
    fn snapshot(&self, throttle: &mut Throttle, changed: bool) {
        if throttle.should_emit(changed, Instant::now()) {
            (self.emit)(BatchEvent::SnapshotChanged);
        }
    }
}

/// Runs the jobs one after another. After a failed or skipped action of a PR, its later jobs are
/// skipped. For `auto` passes, work already done (or already reported as skipped) for the same
/// head commit is left out silently. Runs never overlap (`action_lock`). `emit` hears about the
/// start of the batch, the start of every job and every result, and (throttled, always once
/// after the last change) about changes of the cached snapshot.
pub(crate) async fn execute(
    state: &AppState,
    jobs: Vec<Job>,
    auto: Option<&[Rule]>,
    source: ProgressSource,
    emit: Emit<'_>,
) -> Vec<ActionResult> {
    let Some(first) = jobs.first() else {
        return Vec::new();
    };
    let _serial = state.action_lock.lock().await;
    let batch = Batch {
        id: state.batch_counter.fetch_add(1, Ordering::SeqCst) + 1,
        total: jobs.len(),
        source,
        emit,
    };
    batch.send(0, first, false, None);
    let cached = cached_items(state);
    let fresh = fresh_for(state, &jobs, &cached, auto.is_some()).await;
    let context = Context {
        cached,
        fresh,
        auto,
        rules: lock(&state.rules).value.rules.clone(),
    };
    let mut broken: HashSet<PrRef> = HashSet::new();
    let mut results = Vec::new();
    let mut throttle = Throttle::new(SNAPSHOT_INTERVAL);
    for (index, job) in jobs.into_iter().enumerate() {
        // A change held back by the throttle shows before the next (possibly slow) job.
        batch.snapshot(&mut throttle, false);
        batch.send(index, &job, true, None);
        let skipped = broken
            .contains(&job.pr)
            .then(|| "skipped: an earlier action for this PR did not run".to_owned());
        let ran = run_one(state, &context, &job, skipped).await;
        let changed = ran.as_ref().is_some_and(|(_, changed)| *changed);
        let result = ran.map(|(result, _)| result);
        if let Some(result) = &result {
            if !result.ok {
                broken.insert(job.pr.clone());
            }
            audit(state, &job, result).await;
            results.push(result.clone());
        }
        // Without a result (an auto job handled before) there is nothing to point at.
        batch.send(index + 1, &job, result.is_some(), result);
        batch.snapshot(&mut throttle, changed);
    }
    if throttle.finish() {
        (batch.emit)(BatchEvent::SnapshotChanged);
    }
    results
}

fn cached_items(state: &AppState) -> Items {
    lock(&state.snapshot).as_ref().map_or_default(|snapshot| {
        snapshot
            .items
            .iter()
            .map(|item| (item.pr.clone(), item.clone()))
            .collect()
    })
}

/// Fresh details of the PRs whose jobs need them, with the notification, ownership and
/// duplicate flag of the cached item (the details query does not know them).
async fn fresh_for(
    state: &AppState,
    jobs: &[Job],
    cached: &Items,
    all: bool,
) -> Result<Items, String> {
    let mut seen = HashSet::new();
    let refs: Vec<PrRef> = jobs
        .iter()
        .filter(|job| wants_fresh(&job.action, all) && seen.insert(&job.pr))
        .map(|job| job.pr.clone())
        .collect();
    if refs.is_empty() {
        return Ok(Items::new());
    }
    // A PR GitHub cannot show is missing from the items; its job then reports "no longer open".
    let details = state
        .client
        .pr_details(&refs)
        .await
        .map_err(|error| error.to_string())?;
    Ok(details
        .items
        .into_iter()
        .map(|mut item| {
            if let Some(old) = cached.get(&item.pr) {
                item.notification.clone_from(&old.notification);
                item.is_mine = old.is_mine;
                item.duplicate_of.clone_from(&old.duplicate_of);
            }
            (item.pr.clone(), item)
        })
        .collect())
}

/// The result and whether it changed the cached snapshot; `None`: nothing to report (an auto
/// job that was handled before).
async fn run_one(
    state: &AppState,
    context: &Context<'_>,
    job: &Job,
    skipped: Option<String>,
) -> Option<(ActionResult, bool)> {
    let item = match context.item(job) {
        Ok(item) => item,
        Err(message) => return Some((failure(job, message), false)),
    };
    let auto = context.auto.is_some();
    let key = memo_key(item, job);
    if auto && already_handled(state, &key) {
        return None;
    }
    if let Some(message) = skipped.or_else(|| context.gate(job, item).err()) {
        let first = !auto || lock(&state.memo).skipped.insert(format!("{key}|{message}"));
        return first.then(|| (failure(job, message), false));
    }
    let result = state.client.perform(item, &job.action).await;
    let changed = result.ok && apply_locally(state, item, &job.action);
    if auto {
        remember(state, key, result.ok).await;
    }
    Some((result, changed))
}

/// Updates the cached snapshot right after a successful action, and remembers what GitHub may
/// not report yet (a refresh must not bring the item back). Whether the snapshot changed.
fn apply_locally(state: &AppState, item: &PrItem, action: &Action) -> bool {
    let now = Instant::now();
    {
        let mut overrides = lock(&state.overrides);
        match (action, &item.notification) {
            (Action::MarkDone | Action::Unsubscribe, Some(notification)) => {
                overrides.hide_thread(&notification.thread_id, &notification.updated_at, now);
            }
            (Action::Merge(_) | Action::ApproveAndMerge(_), _) => {
                overrides.remember_state(&item.pr, PrState::Merged, &item.updated_at, now);
            }
            (Action::Close { .. }, _) => {
                overrides.remember_state(&item.pr, PrState::Closed, &item.updated_at, now);
            }
            _ => {}
        }
    }
    lock(&state.snapshot)
        .as_mut()
        .is_some_and(|snapshot| snapshot.apply_action(&item.pr, action))
}

fn already_handled(state: &AppState, key: &str) -> bool {
    let memo = lock(&state.memo);
    memo.executed.contains(key) || memo.failed.contains(key)
}

/// Successes are persisted (a restart must not repeat them); failures are not retried in this
/// session.
async fn remember(state: &AppState, key: String, ok: bool) {
    let json = {
        let mut memo = lock(&state.memo);
        if !ok {
            memo.failed.insert(key);
            return;
        }
        memo.executed.insert(key);
        memo.executed_json()
    };
    // Losing the memo file only risks a repeated action, which the approval check and the
    // audit log make visible; it must not fail the action. Batches never overlap, so the
    // writes happen in order.
    if let Some(json) = json {
        let dir = state.config_dir.clone();
        let _ = config::blocking(move || config::write_memo(&dir, &json)).await;
    }
}

/// Identifies an `auto` action. Comments (also closing comments) are keyed without the head
/// commit, so a new push does not make a rule comment again; everything else is per head.
fn memo_key(item: &PrItem, job: &Job) -> String {
    let head = match &job.action {
        Action::Comment(_) | Action::Close { comment: Some(_) } => "",
        _ => item.head_oid.as_str(),
    };
    format!("{}|{head}|{}", job.pr, job.action)
}

fn failure(job: &Job, message: String) -> ActionResult {
    ActionResult {
        pr: job.pr.clone(),
        action: job.action.clone(),
        ok: false,
        message,
    }
}

/// Appends to `audit.jsonl`.
async fn audit(state: &AppState, job: &Job, result: &ActionResult) {
    let entry = AuditEntry {
        at: now_rfc3339(),
        rule: job.rule.as_ref().map(|rule| rule.name.clone()),
        result: result.clone(),
    };
    let dir = state.config_dir.clone();
    // An audit failure must not undo or hide an executed action.
    let _ = config::blocking(move || config::append_audit(&dir, &entry)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    use prism_rules::ChecksReq;
    use std::sync::Mutex;

    fn item(head: &str) -> PrItem {
        let mut item = PrItem::sample("a/b", 1, "Bump serde from 1.0.1 to 1.0.2");
        item.head_oid = head.to_owned();
        item
    }

    fn pr() -> PrRef {
        item("h").pr
    }

    fn rule(name: &str, title: &str) -> Rule {
        Rule {
            name: name.to_owned(),
            title: Some(title.to_owned()),
            checks: ChecksReq::Any,
            actions: vec![Action::Merge(None)],
            ..Rule::default()
        }
    }

    fn manual(action: Action, head: Option<String>) -> Job {
        Job::manual(ActTarget { pr: pr(), head }, action)
    }

    fn context(cached: &str, rules: Vec<Rule>) -> Context<'static> {
        Context {
            cached: [(pr(), item(cached))].into_iter().collect(),
            fresh: Ok(Items::new()),
            auto: None,
            rules,
        }
    }

    #[test]
    fn approving_and_merging_actions_need_fresh_details() {
        assert!(needs_fresh(&Action::Merge(None)));
        assert!(needs_fresh(&Action::ApproveAndMerge(None)));
        assert!(needs_fresh(&Action::EnableAutoMerge(None)));
        assert!(needs_fresh(&Action::Approve));
        assert!(needs_fresh(&Action::Rebase));
        assert!(!needs_fresh(&Action::MarkDone));
        assert!(!needs_fresh(&Action::Close { comment: None }));
    }

    #[test]
    fn merged_and_closed_prs_only_allow_notification_actions() {
        let context = context("h", Vec::new());
        for state in [PrState::Merged, PrState::Closed] {
            let mut pr_item = item("h");
            pr_item.state = state;
            for action in [
                Action::Approve,
                Action::Merge(None),
                Action::Rebase,
                Action::Close { comment: None },
                Action::Comment("x".to_owned()),
                Action::RerunFailed,
            ] {
                let job = manual(action, None);
                assert!(context.gate(&job, &pr_item).is_err());
            }
            for action in [Action::MarkRead, Action::MarkDone, Action::Unsubscribe] {
                let job = manual(action, None);
                assert!(context.gate(&job, &pr_item).is_ok());
            }
        }
        assert!(!wants_fresh(&Action::MarkRead, true));
        assert!(wants_fresh(&Action::Close { comment: None }, true));
        assert!(!wants_fresh(&Action::Close { comment: None }, false));
    }

    #[test]
    fn unavailable_actions_are_refused_with_the_reason() {
        let context = context("h", Vec::new());
        let mut pr_item = item("h");
        pr_item.caps.auto_merge = protocol::AutoMerge::Unavailable("disabled here".to_owned());
        let auto_merge = manual(Action::EnableAutoMerge(None), None);
        let error = context.gate(&auto_merge, &pr_item).unwrap_err();
        assert!(error.contains("disabled here"), "{error}");
        pr_item.is_mine = true;
        let approve = manual(Action::Approve, None);
        assert!(
            context
                .gate(&approve, &pr_item)
                .unwrap_err()
                .contains("own")
        );
        let merge = manual(Action::Merge(None), None);
        assert!(context.gate(&merge, &pr_item).is_ok());
    }

    #[test]
    fn manual_merge_refuses_a_moved_head() {
        let context = context("old", Vec::new());
        let merge = manual(Action::Merge(None), None);
        assert!(context.gate(&merge, &item("old")).is_ok());
        assert!(context.gate(&merge, &item("new")).is_err());
        let seen = manual(Action::Merge(None), Some("new".to_owned()));
        assert!(context.gate(&seen, &item("new")).is_ok());
        assert!(context.gate(&seen, &item("old")).is_err());
        let rebase = manual(Action::Rebase, Some("old".to_owned()));
        assert!(context.gate(&rebase, &item("old")).is_ok());
        assert!(context.gate(&rebase, &item("new")).is_err());
        // Not head-bound: no head check.
        let close = manual(Action::Close { comment: None }, None);
        assert!(context.gate(&close, &item("new")).is_ok());
    }

    #[test]
    fn applied_suggestion_rechecks_its_rule() {
        let rules = vec![rule("bumps", "^Bump "), rule("bumps", "^Never")];
        let context = context("h", rules);
        let planned = |rule: &str, rule_index: usize| {
            Job::planned(PlannedAction {
                rule: rule.to_owned(),
                rule_index,
                pr: pr(),
                action: Action::Merge(None),
                head_oid: "h".to_owned(),
            })
        };
        assert!(context.gate(&planned("bumps", 0), &item("h")).is_ok());
        let mut renamed = item("h");
        renamed.title = "Something else".to_owned();
        assert!(context.gate(&planned("bumps", 0), &renamed).is_err());
        assert!(context.gate(&planned("gone", 0), &item("h")).is_err());
        assert!(context.gate(&planned("bumps", 5), &item("h")).is_err());
        assert!(context.gate(&planned("bumps", 0), &item("moved")).is_err());
        // The second rule of the same name is checked, not the first.
        let error = context.gate(&planned("bumps", 1), &item("h")).unwrap_err();
        assert!(error.contains("no longer matches"), "{error}");
    }

    #[test]
    fn comment_memo_ignores_the_head() {
        let comment = manual(Action::Comment("hi".to_owned()), None);
        assert_eq!(
            memo_key(&item("a"), &comment),
            memo_key(&item("b"), &comment)
        );
        let approve = manual(Action::Approve, None);
        assert_ne!(
            memo_key(&item("a"), &approve),
            memo_key(&item("b"), &approve)
        );
    }

    #[tokio::test]
    async fn emits_start_then_job_start_and_result_events() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(dir.path().to_owned()).unwrap();
        let events = Mutex::new(Vec::new());
        let emit = |event: BatchEvent| match event {
            BatchEvent::Progress(progress) => events.lock().unwrap().push(progress),
            BatchEvent::SnapshotChanged => panic!("nothing changed"),
        };
        // Nothing is cached, so both jobs fail locally without touching the network.
        let jobs = vec![
            manual(Action::MarkDone, None),
            manual(Action::MarkRead, None),
        ];
        let results = execute(&state, jobs, None, ProgressSource::Manual, &emit).await;
        assert_eq!(results.len(), 2);
        let seen = events.lock().unwrap().clone();
        let shape: Vec<_> = seen
            .iter()
            .map(|e| (e.done, e.current.is_some(), e.last.is_some()))
            .collect();
        assert_eq!(
            shape,
            [
                (0, false, false),
                (0, true, false),
                (1, true, true),
                (1, true, false),
                (2, true, true),
            ]
        );
        assert!(seen.iter().all(|e| e.total == 2 && e.batch_id == 1));
        assert_eq!(seen[4].last.as_ref(), results.last());
        let none = execute(&state, Vec::new(), None, ProgressSource::Manual, &emit).await;
        assert!(none.is_empty());
        assert_eq!(events.lock().unwrap().len(), 5, "empty batches stay silent");
        // Both failures are audited.
        let audited = config::read_audit(dir.path(), 10);
        assert_eq!(audited.len(), 2);
    }

    #[test]
    fn snapshot_announcements_are_throttled_with_leading_and_trailing_emits() {
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let mut throttle = Throttle::new(Duration::from_millis(250));
        assert!(!throttle.should_emit(false, at(0)), "no change, no emit");
        assert!(
            throttle.should_emit(true, at(0)),
            "the first change at once"
        );
        assert!(!throttle.should_emit(true, at(100)), "held back");
        assert!(!throttle.should_emit(false, at(200)));
        assert!(
            throttle.should_emit(false, at(250)),
            "trailing emit once due"
        );
        assert!(!throttle.should_emit(false, at(600)), "nothing pending");
        assert!(throttle.should_emit(true, at(600)), "due again: at once");
        assert!(!throttle.should_emit(true, at(700)));
        assert!(throttle.finish(), "the end announces what is pending");
        assert!(!throttle.finish());
    }

    #[test]
    fn memo_is_bounded_and_deduplicated() {
        let mut memo = AutoMemo::new(vec!["a".to_owned(), "a".to_owned()]);
        assert_eq!(memo.executed_json().unwrap(), r#"["a"]"#);
        assert!(!memo.executed.insert("a".to_owned()));
        for index in 0..MEMO_LIMIT {
            memo.executed.insert(index.to_string());
        }
        assert_eq!(memo.executed.order.len(), MEMO_LIMIT);
        assert_eq!(memo.executed.members.len(), MEMO_LIMIT);
        assert!(!memo.executed.contains("a"));
        assert!(memo.executed.contains("0"));
    }

    fn loaded_state(items: Vec<PrItem>) -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(dir.path().to_owned()).unwrap();
        *lock(&state.snapshot) = Some(protocol::InboxSnapshot {
            items,
            fetched_at: String::new(),
            error: None,
        });
        (dir, state)
    }

    #[test]
    fn local_application_patches_the_snapshot_and_remembers_overrides() {
        let mut notified = item("h");
        notified.notification = Some(protocol::NotificationInfo {
            thread_id: "t1".to_owned(),
            reason: protocol::NotifyReason::Mention,
            unread: true,
            updated_at: "u".to_owned(),
        });
        let (_dir, state) = loaded_state(vec![notified.clone()]);
        // Approving changes nothing locally.
        assert!(!apply_locally(&state, &notified, &Action::Approve));
        assert!(apply_locally(&state, &notified, &Action::MarkDone));
        assert!(lock(&state.snapshot).as_ref().unwrap().items.is_empty());
        // A refresh that still reports the thread does not bring it back.
        let mut fetched = vec![notified.clone()];
        lock(&state.overrides).apply(&mut fetched, Instant::now());
        assert!(fetched.is_empty());

        let (_dir, state) = loaded_state(vec![item("h")]);
        assert!(apply_locally(&state, &item("h"), &Action::Merge(None)));
        let mut lagging = vec![item("h")];
        lock(&state.overrides).apply(&mut lagging, Instant::now());
        assert!(lagging.is_empty(), "merged without another source: gone");
        // Not in the snapshot: nothing changes.
        let mut other = item("h");
        other.pr.number = 2;
        assert!(!apply_locally(&state, &other, &Action::MarkRead));
    }
}
