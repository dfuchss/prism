use super::*;
use protocol::{Bump, CheckRun, CheckState, MergeMethod, NotificationInfo, NotifyReason, PrRef};

/// An open, green bot PR that involves the viewer (a review request) and is no bump.
fn item(repo: &str, number: u64, title: &str) -> PrItem {
    let mut item = PrItem::sample(repo, number, title);
    item.checks = vec![check(CheckState::Success)];
    item.review_requested = true;
    item
}

/// Like [`item`], a bump of the given kind.
fn bump(repo: &str, number: u64, kind: UpdateKind) -> PrItem {
    let mut item = item(repo, number, "Bump serde");
    item.bump = Some(Bump {
        dependency: "serde".to_owned(),
        from: Some("1.0.1".to_owned()),
        to: "1.1.0".to_owned(),
        kind,
        directory: None,
    });
    item
}

fn minor(number: u64) -> PrItem {
    bump("a/b", number, UpdateKind::Minor)
}

fn check(state: CheckState) -> CheckRun {
    CheckRun {
        name: "ci".to_owned(),
        state,
        details_url: None,
        workflow_run_id: None,
    }
}

fn rule(name: &str) -> Rule {
    Rule {
        name: name.to_owned(),
        checks: ChecksReq::Any,
        actions: vec![Action::Approve],
        ..Rule::default()
    }
}

#[test]
fn empty_filters_match_everything_but_drafts() {
    let r = rule("all");
    assert!(matches(&r, &item("a/b", 1, "anything")));
    let mut draft = item("a/b", 2, "anything");
    draft.draft = true;
    assert!(!matches(&r, &draft));
    let mut state_draft = item("a/b", 3, "anything");
    state_draft.merge_state = MergeState::Draft;
    assert!(!matches(&r, &state_draft));
}

#[test]
fn authors_ignore_case() {
    let mut r = rule("bots");
    r.authors = vec!["dependabot[bot]".to_owned(), "renovate[bot]".to_owned()];
    let mut pr = item("a/b", 1, "t");
    assert!(matches(&r, &pr));
    pr.author = "renovate".to_owned();
    assert!(!matches(&r, &pr));
    pr.author = "Renovate[bot]".to_owned();
    assert!(matches(&r, &pr));
}

#[test]
fn repos_use_globs() {
    let mut r = rule("repos");
    r.repos = vec!["acme/*".to_owned(), "org/**".to_owned()];
    assert!(matches(&r, &item("acme/widget", 1, "t")));
    assert!(matches(&r, &item("org/a/b", 1, "t")));
    assert!(!matches(&r, &item("other/prism", 1, "t")));
}

#[test]
fn title_regex() {
    let mut r = rule("title");
    r.title = Some(r"^Bump .* from".to_owned());
    assert!(matches(
        &r,
        &item("a/b", 1, "Bump serde from 1.0.1 to 1.1.0")
    ));
    assert!(!matches(&r, &item("a/b", 1, "Update x to v1")));
}

#[test]
fn invalid_regex_never_matches_and_is_reported() {
    let mut r = rule("broken");
    r.title = Some("(unclosed".to_owned());
    assert!(!matches(&r, &item("a/b", 1, "(unclosed")));
    assert!(evaluate(&[r.clone()], &[item("a/b", 1, "x")]).is_empty());
    let problems = validate(&[rule("fine"), r]);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("broken") && problems[0].contains("regex"));
}

#[test]
fn validate_reports_missing_actions_duplicate_names_and_useless_globs() {
    let mut r = rule("empty");
    r.actions.clear();
    assert_eq!(validate(&[r]), ["rule `empty`: no actions"]);
    assert!(validate(&[rule("ok"), rule("other")]).is_empty());
    let twice = validate(&[rule("same"), rule("ok"), rule("same")]);
    assert_eq!(twice.len(), 1);
    assert!(twice[0].contains("`same`") && twice[0].contains("more than once"));
    let mut globs = rule("globs");
    globs.repos = vec!["acme".to_owned(), "acme/*".to_owned()];
    let problems = validate(&[globs]);
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("repos `acme` never matches"),
        "{problems:?}"
    );
}

#[test]
fn update_kinds() {
    let mut r = rule("kinds");
    r.update_kinds = vec![UpdateKind::Patch, UpdateKind::Minor];
    assert!(matches(&r, &minor(1)));
    assert!(matches(&r, &bump("a/b", 1, UpdateKind::Patch)));
    assert!(!matches(&r, &bump("a/b", 1, UpdateKind::Major)));
    assert!(!matches(&r, &bump("a/b", 1, UpdateKind::Unknown)));
    // No bump at all counts as unknown.
    assert!(!matches(&r, &item("a/b", 1, "Fix the build")));
    r.update_kinds = vec![UpdateKind::Unknown];
    assert!(matches(&r, &item("a/b", 1, "Fix the build")));
    assert!(matches(&r, &bump("a/b", 1, UpdateKind::Unknown)));
}

#[test]
fn green_checks() {
    let mut r = rule("green");
    r.checks = ChecksReq::Green;
    let mut pr = item("a/b", 1, "t");
    assert!(matches(&r, &pr));
    pr.checks = vec![check(CheckState::Success), check(CheckState::Failure)];
    assert!(!matches(&r, &pr));
    pr.checks = vec![check(CheckState::Pending)];
    assert!(!matches(&r, &pr));
    pr.checks.clear();
    assert!(matches(&r, &pr), "no checks + clean is fine");
    pr.merge_state = MergeState::Blocked;
    assert!(!matches(&r, &pr), "no checks + blocked is not");
    r.checks = ChecksReq::Any;
    pr.checks = vec![check(CheckState::Failure)];
    assert!(matches(&r, &pr));
}

#[test]
fn duplicate_filter() {
    let mut dup = item("a/b", 1, "t");
    dup.duplicate_of = Some(PrRef {
        repo: "a/b".to_owned(),
        number: 2,
    });
    let plain = item("a/b", 2, "t");
    let mut r = rule("dups");
    r.duplicate = DuplicateFilter::OnlyDuplicates;
    assert!(matches(&r, &dup) && !matches(&r, &plain));
    r.duplicate = DuplicateFilter::NoDuplicates;
    assert!(!matches(&r, &dup) && matches(&r, &plain));
    r.duplicate = DuplicateFilter::Any;
    assert!(matches(&r, &dup) && matches(&r, &plain));
}

#[test]
fn involved_filter() {
    let mine = item("a/b", 1, "t");
    let scoped = scope_only(item("a/b", 2, "t"));
    let mut r = rule("involved");
    r.involved = Involvement::Involved;
    assert!(matches(&r, &mine) && !matches(&r, &scoped));
    r.involved = Involvement::ScopeOnly;
    assert!(!matches(&r, &mine) && matches(&r, &scoped));
    r.involved = Involvement::Any;
    assert!(matches(&r, &mine) && matches(&r, &scoped));
}

#[test]
fn evaluate_orders_actions_and_dedupes() {
    let mut first = rule("first");
    first.actions = vec![Action::Approve, Action::Merge(None)];
    let mut second = rule("second");
    second.actions = vec![
        Action::Merge(None),
        Action::MarkDone,
        Action::Merge(Some(MergeMethod::Squash)),
    ];
    let items = [notified(minor(1), true), notified(minor(2), true)];
    let planned = evaluate(&[first, second], &items);
    let summary: Vec<(&str, usize, u64, &Action)> = planned
        .iter()
        .map(|p| (p.rule.as_str(), p.rule_index, p.pr.number, &p.action))
        .collect();
    assert_eq!(
        summary,
        [
            ("first", 0, 1, &Action::Approve),
            ("first", 0, 1, &Action::Merge(None)),
            ("first", 0, 2, &Action::Approve),
            ("first", 0, 2, &Action::Merge(None)),
            ("second", 1, 1, &Action::MarkDone),
            ("second", 1, 1, &Action::Merge(Some(MergeMethod::Squash))),
            ("second", 1, 2, &Action::MarkDone),
            ("second", 1, 2, &Action::Merge(Some(MergeMethod::Squash))),
        ]
    );
}

#[test]
fn evaluate_skips_non_matching_and_drafts() {
    let mut r = rule("r");
    r.repos = vec!["a/*".to_owned()];
    let mut draft = item("a/b", 3, "t");
    draft.draft = true;
    let items = [item("a/b", 1, "t"), item("c/d", 2, "t"), draft];
    let planned = evaluate(&[r], &items);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].pr.number, 1);
}

#[test]
fn evaluate_never_plans_unavailable_actions() {
    let mut r = rule("r");
    r.actions = vec![
        Action::Approve,
        Action::EnableAutoMerge(None),
        Action::MarkDone,
    ];
    let mut off = item("a/b", 1, "t");
    off.caps.auto_merge = protocol::AutoMerge::Unavailable("disabled".to_owned());
    let off = notified(off, true);
    let mut own = item("a/b", 2, "t");
    own.is_mine = true;
    let planned = evaluate(&[r], &[off, own]);
    let plan: Vec<_> = planned.iter().map(|p| (p.pr.number, &p.action)).collect();
    assert_eq!(
        plan,
        [
            (1, &Action::Approve),
            (1, &Action::MarkDone),
            (2, &Action::EnableAutoMerge(None)),
        ]
    );
}

#[test]
fn auto_safety() {
    let r = rule("auto");
    let merge = Action::Merge(None);
    let approve_merge = Action::ApproveAndMerge(Some(MergeMethod::Squash));
    let auto_merge = Action::EnableAutoMerge(None);
    let major = bump("a/b", 1, UpdateKind::Major);
    let unknown = bump("a/b", 1, UpdateKind::Unknown);
    let no_bump = item("a/b", 1, "Fix");
    for action in [&merge, &approve_merge, &auto_merge] {
        assert!(auto_allowed(&r, &minor(1), action));
        assert!(auto_allowed(&r, &bump("a/b", 1, UpdateKind::Patch), action));
        for refused in [&major, &unknown, &no_bump] {
            assert!(!auto_allowed(&r, refused, action));
        }
        let mut failing = minor(1);
        failing.checks = vec![check(CheckState::Failure)];
        assert!(!auto_allowed(&r, &failing, action));
        let mut no_checks = minor(1);
        no_checks.checks.clear();
        assert!(!auto_allowed(&r, &no_checks, action));
        let mut draft = minor(1);
        draft.draft = true;
        assert!(!auto_allowed(&r, &draft, action));
        let mut pending = minor(1);
        pending.checks.push(check(CheckState::Pending));
        assert!(!auto_allowed(&r, &pending, action));
    }
    // Non-merging actions are not gated.
    assert!(auto_allowed(&r, &major, &Action::Approve));
    assert!(auto_allowed(&r, &no_bump, &Action::MarkDone));
}

const EXAMPLE_FILES: [(&str, &str); 7] = [
    (
        "approve-green-bot-prs",
        include_str!("../../../examples/rules/approve-green-bot-prs.toml"),
    ),
    (
        "merge-green-patch-minor",
        include_str!("../../../examples/rules/merge-green-patch-minor.toml"),
    ),
    (
        "rebase-stale-bot-prs",
        include_str!("../../../examples/rules/rebase-stale-bot-prs.toml"),
    ),
    (
        "close-duplicates",
        include_str!("../../../examples/rules/close-duplicates.toml"),
    ),
    (
        "mark-merged-read",
        include_str!("../../../examples/rules/mark-merged-read.toml"),
    ),
    (
        "scope-only-safety",
        include_str!("../../../examples/rules/scope-only-safety.toml"),
    ),
    (
        "settings.example",
        include_str!("../../../examples/rules/settings.example.toml"),
    ),
];

fn example(name: &str) -> RuleSet {
    let (_, text) = EXAMPLE_FILES.iter().find(|(n, _)| *n == name).unwrap();
    load_rules(text).unwrap()
}

#[test]
fn example_files_parse_and_validate() {
    let on_disk = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/rules"))
        .unwrap()
        .count();
    assert_eq!(on_disk, EXAMPLE_FILES.len(), "list every example file here");
    for (name, text) in EXAMPLE_FILES {
        if name == "settings.example" {
            // Parsed and validated by the app, which owns settings.toml.
            continue;
        }
        let set = load_rules(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!set.rules.is_empty(), "{name}");
        assert!(validate(&set.rules).is_empty(), "{name}");
    }
}

#[test]
fn default_rules_load_validate_and_are_passive() {
    let set = load_rules(default_rules_toml()).unwrap();
    assert!(set.rules.is_empty());
    assert!(validate(&set.rules).is_empty());
    assert!(!default_rules_toml().contains("\nauto = true"));
}

#[test]
fn example_merge_rule_plans_approve_and_merge() {
    let set = example("merge-green-patch-minor");
    let pr = bump("acme/x", 1, UpdateKind::Minor);
    let planned = evaluate(&set.rules, std::slice::from_ref(&pr));
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].action, Action::ApproveAndMerge(None));
    assert!(auto_allowed(&set.rules[0], &pr, &planned[0].action));
}

#[test]
fn load_rules_reports_errors() {
    assert!(load_rules("[[rule]]\nname = \"x\"\nactions = [\"explode\"]\n").is_err());
    assert!(load_rules("this is not toml").is_err());
    assert_eq!(load_rules("").unwrap().rules.len(), 0);
}

#[test]
fn state_filter_selects_merge_states() {
    let mut r = rule("stale");
    r.merge_state = vec![MergeState::Behind, MergeState::Dirty];
    let mut pr = item("a/b", 1, "t");
    assert!(!matches(&r, &pr));
    pr.merge_state = MergeState::Behind;
    assert!(matches(&r, &pr));
    pr.merge_state = MergeState::Dirty;
    assert!(matches(&r, &pr));
    pr.merge_state = MergeState::Blocked;
    assert!(!matches(&r, &pr));
}

#[test]
fn rebase_example_works_with_failing_checks() {
    let set = example("rebase-stale-bot-prs");
    assert_eq!(set.rules.len(), 1);
    let mut pr = item("a/b", 1, "t");
    pr.author = "dependabot[bot]".to_owned();
    pr.merge_state = MergeState::Dirty;
    pr.checks = vec![check(CheckState::Failure)];
    let planned = evaluate(&set.rules, &[pr.clone()]);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].action, Action::Rebase);
    assert!(auto_allowed(&set.rules[0], &pr, &Action::Rebase));
}

#[test]
fn rebase_is_auto_allowed_except_for_drafts() {
    let r = rule("r");
    assert!(auto_allowed(&r, &item("a/b", 1, "Fix"), &Action::Rebase));
    let mut draft = item("a/b", 1, "Fix");
    draft.draft = true;
    assert!(!auto_allowed(&r, &draft, &Action::Rebase));
    let mut state_draft = item("a/b", 1, "Fix");
    state_draft.merge_state = MergeState::Draft;
    assert!(!auto_allowed(&r, &state_draft, &Action::Rebase));
}

#[test]
fn auto_refuses_scope_only_prs_unless_involved_is_false() {
    let scoped = scope_only(minor(1));
    let mut r = rule("r");
    for action in [Action::Merge(None), Action::Approve, Action::Rebase] {
        r.involved = Involvement::Any;
        assert!(!auto_allowed(&r, &scoped, &action));
        r.involved = Involvement::Involved;
        assert!(!auto_allowed(&r, &scoped, &action));
        r.involved = Involvement::ScopeOnly;
        assert!(auto_allowed(&r, &scoped, &action));
    }
    // Suggestions still match.
    r.involved = Involvement::Any;
    assert!(matches(&r, &scoped));
    assert_eq!(evaluate(&[r.clone()], &[scoped]).len(), 1);
    // Involved PRs are unaffected.
    assert!(auto_allowed(&r, &minor(2), &Action::Merge(None)));
}

fn notified(mut pr: PrItem, unread: bool) -> PrItem {
    pr.notification = Some(NotificationInfo {
        thread_id: "1".to_owned(),
        reason: NotifyReason::Subscribed,
        unread,
        updated_at: String::new(),
    });
    pr
}

/// In the inbox only through the repository scope.
fn scope_only(mut pr: PrItem) -> PrItem {
    pr.review_requested = false;
    pr.in_scope = true;
    pr
}

fn merged(mut pr: PrItem) -> PrItem {
    pr.state = PrState::Merged;
    pr
}

#[test]
fn pr_state_filter_defaults_to_open() {
    let mut r = rule("r");
    let open = minor(1);
    let merged = merged(minor(2));
    assert!(matches(&r, &open) && !matches(&r, &merged));
    r.pr_state = vec![PrState::Merged];
    assert!(!matches(&r, &open) && matches(&r, &merged));
    r.pr_state = vec![PrState::Open, PrState::Merged];
    assert!(matches(&r, &open) && matches(&r, &merged));
}

#[test]
fn planner_skips_noop_notification_actions() {
    let mut r = rule("r");
    r.actions = vec![Action::MarkRead, Action::MarkDone, Action::Unsubscribe];
    let count = |pr: PrItem| evaluate(&[r.clone()], &[pr]).len();
    assert_eq!(count(minor(1)), 0);
    assert_eq!(count(notified(minor(1), false)), 2);
    assert_eq!(count(notified(minor(1), true)), 3);
}

#[test]
fn auto_allows_only_notification_actions_on_non_open_prs() {
    let r = rule("r");
    let all = [
        Action::Approve,
        Action::Merge(None),
        Action::ApproveAndMerge(None),
        Action::EnableAutoMerge(None),
        Action::Close { comment: None },
        Action::Comment("x".to_owned()),
        Action::RerunFailed,
        Action::Rebase,
    ];
    for state in [PrState::Merged, PrState::Closed] {
        let mut pr = minor(1);
        pr.state = state;
        for action in &all {
            assert!(!auto_allowed(&r, &pr, action), "{state:?} {action:?}");
        }
        for action in [Action::MarkRead, Action::MarkDone, Action::Unsubscribe] {
            assert!(auto_allowed(&r, &pr, &action), "{state:?} {action:?}");
        }
    }
}

#[test]
fn example_rule_marks_merged_prs_read_once() {
    let set = example("mark-merged-read");
    let rule = set
        .rules
        .iter()
        .find(|r| r.name == "mark merged as read")
        .unwrap();
    assert!(rule.auto && rule.pr_state == [PrState::Merged]);
    let mut failing = merged(notified(item("a/b", 1, "Fix"), true));
    failing.checks = vec![check(CheckState::Failure)];
    failing.merge_state = MergeState::Dirty;
    assert!(auto_allowed(rule, &failing, &Action::MarkRead));
    let planned = evaluate(&set.rules, &[failing]);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].action, Action::MarkRead);
    let read = merged(notified(item("a/b", 2, "Fix"), false));
    assert!(evaluate(&set.rules, &[read]).is_empty());
    // Open PRs are untouched by it.
    let open = notified(item("a/b", 3, "Fix"), true);
    assert!(
        evaluate(&set.rules, &[open])
            .iter()
            .all(|p| p.rule != rule.name)
    );
}
