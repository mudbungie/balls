//! Coverage for the apply-time planner. The planner is pure, so each
//! test is a slice of `Contribution`s and an expected `Vec<PlanOp>`.

use super::*;

fn contrib(name: &str, fp: FailurePolicy, cp: CommitPolicy) -> Contribution {
    Contribution {
        name: name.into(),
        failure_policy: fp,
        commit_policy: cp,
    }
}

const DEFAULT_MSG: &str = "balls: update external for bl-1234";

#[test]
fn empty_contributions_emits_no_ops() {
    let ops = plan(&[], DEFAULT_MSG).unwrap();
    assert!(ops.is_empty());
}

#[test]
fn all_default_commits_collapse_into_single_default_commit() {
    // Regression guard: all legacy plugins return Commit { None }; the
    // planner must produce one save per participant followed by exactly
    // one commit with today's default message. Same observable
    // state-branch result as pre-CommitPolicy dispatch.
    let cs = vec![
        contrib("a", FailurePolicy::BestEffort, CommitPolicy::default()),
        contrib("b", FailurePolicy::BestEffort, CommitPolicy::default()),
    ];
    assert_eq!(
        plan(&cs, DEFAULT_MSG).unwrap(),
        vec![
            PlanOp::Apply(0),
            PlanOp::Apply(1),
            PlanOp::Commit(DEFAULT_MSG.into()),
        ]
    );
}

#[test]
fn suppress_on_best_effort_defers_to_default_commit() {
    // SPEC §10: Suppress applies state but does not cause its own
    // commit. The trailing default commit picks it up.
    let cs = vec![contrib(
        "a",
        FailurePolicy::BestEffort,
        CommitPolicy::Suppress,
    )];
    assert_eq!(
        plan(&cs, DEFAULT_MSG).unwrap(),
        vec![PlanOp::Apply(0), PlanOp::Commit(DEFAULT_MSG.into())]
    );
}

#[test]
fn suppress_followed_by_commit_some_lets_next_commit_capture() {
    // SPEC §10: "the next participant's Commit picks it up." Apply
    // ops are sequential and a later Commit captures whatever has
    // been saved so far. The planner emits no extra default commit
    // because the explicit Commit covers the suppressed state.
    let cs = vec![
        contrib("a", FailurePolicy::BestEffort, CommitPolicy::Suppress),
        contrib(
            "b",
            FailurePolicy::BestEffort,
            CommitPolicy::Commit { message: Some("hello".into()) },
        ),
    ];
    assert_eq!(
        plan(&cs, DEFAULT_MSG).unwrap(),
        vec![
            PlanOp::Apply(0),
            PlanOp::Apply(1),
            PlanOp::Commit("plugin: b: hello".into()),
        ]
    );
}

#[test]
fn suppress_on_required_errors_before_any_apply() {
    let cs = vec![
        contrib("a", FailurePolicy::BestEffort, CommitPolicy::default()),
        contrib("b", FailurePolicy::Required, CommitPolicy::Suppress),
    ];
    let err = plan(&cs, DEFAULT_MSG).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Suppress"), "{msg}");
    assert!(msg.contains("Required"), "{msg}");
    assert!(msg.contains('b'), "{msg}");
}

#[test]
fn batch_three_participants_coalesces_into_one_commit() {
    // SPEC §17 conformance test 11. Three participants, one tag, one
    // commit referencing all three. The default-commit fallback is
    // suppressed because the batch flush also captures any
    // suppressed/deferred state from the same event.
    let cs = vec![
        contrib(
            "a",
            FailurePolicy::BestEffort,
            CommitPolicy::Batch { tag: "x".into() },
        ),
        contrib(
            "b",
            FailurePolicy::BestEffort,
            CommitPolicy::Batch { tag: "x".into() },
        ),
        contrib(
            "c",
            FailurePolicy::BestEffort,
            CommitPolicy::Batch { tag: "x".into() },
        ),
    ];
    assert_eq!(
        plan(&cs, DEFAULT_MSG).unwrap(),
        vec![
            PlanOp::Apply(0),
            PlanOp::Apply(1),
            PlanOp::Apply(2),
            PlanOp::Commit("balls: batch x\n\nparticipants: a, b, c".into()),
        ]
    );
}

#[test]
fn batch_flush_captures_deferred_suppress_state() {
    // SPEC §10: "If Batch and Suppress outcomes from disjoint
    // participants coexist, the Batch flush at end-of-event commits
    // the suppressed state too." The planner must NOT emit a trailing
    // default commit when any batch fires.
    let cs = vec![
        contrib("a", FailurePolicy::BestEffort, CommitPolicy::Suppress),
        contrib(
            "b",
            FailurePolicy::BestEffort,
            CommitPolicy::Batch { tag: "x".into() },
        ),
    ];
    assert_eq!(
        plan(&cs, DEFAULT_MSG).unwrap(),
        vec![
            PlanOp::Apply(0),
            PlanOp::Apply(1),
            PlanOp::Commit("balls: batch x\n\nparticipants: b".into()),
        ]
    );
}

#[test]
fn batch_with_distinct_tags_emits_one_commit_per_tag() {
    let cs = vec![
        contrib(
            "a",
            FailurePolicy::BestEffort,
            CommitPolicy::Batch { tag: "audit".into() },
        ),
        contrib(
            "b",
            FailurePolicy::BestEffort,
            CommitPolicy::Batch { tag: "ship".into() },
        ),
    ];
    let ops = plan(&cs, DEFAULT_MSG).unwrap();
    assert_eq!(ops.len(), 4);
    assert_eq!(ops[0], PlanOp::Apply(0));
    assert_eq!(ops[1], PlanOp::Apply(1));
    // BTreeMap iteration is alphabetical: audit before ship.
    assert_eq!(
        ops[2],
        PlanOp::Commit("balls: batch audit\n\nparticipants: a".into())
    );
    assert_eq!(
        ops[3],
        PlanOp::Commit("balls: batch ship\n\nparticipants: b".into())
    );
}

#[test]
fn commit_some_emits_immediately_with_plugin_prefix() {
    let cs = vec![contrib(
        "mock",
        FailurePolicy::BestEffort,
        CommitPolicy::Commit { message: Some("Synced MOCK-1".into()) },
    )];
    assert_eq!(
        plan(&cs, DEFAULT_MSG).unwrap(),
        vec![
            PlanOp::Apply(0),
            PlanOp::Commit("plugin: mock: Synced MOCK-1".into()),
        ]
    );
}

#[test]
fn plugin_commit_message_preserves_body_below_title() {
    // 50/72 shape: first line of the participant's content rides
    // along with the prefix as the title; subsequent lines are the
    // commit body.
    let m = plugin_commit_message("jira", "Sync PROJ-1\n\nDetails about the sync.");
    assert_eq!(m, "plugin: jira: Sync PROJ-1\n\nDetails about the sync.");
}

#[test]
fn plugin_commit_message_handles_single_line() {
    assert_eq!(plugin_commit_message("mock", "custom"), "plugin: mock: custom");
}

#[test]
fn plugin_commit_message_strips_only_blank_separator() {
    // Blank lines between the title and a body collapse to a single
    // blank separator; multiple blank lines below that round-trip
    // unchanged in the body.
    let m = plugin_commit_message("p", "T\n\n\nbody1\n\nbody2");
    assert_eq!(m, "plugin: p: T\n\nbody1\n\nbody2");
}

#[test]
fn plugin_commit_message_empty_input_yields_prefix_only() {
    // A plugin that returns Some("") is asking for an empty title.
    // The wrapper preserves attribution but produces no body.
    assert_eq!(plugin_commit_message("p", ""), "plugin: p: ");
}

#[test]
fn plugin_commit_message_blank_only_body_drops_to_prefix() {
    // Title plus only blank lines below produces a prefix-only commit
    // — a plugin that can't even fill in a body shouldn't get a body.
    assert_eq!(plugin_commit_message("p", "T\n\n"), "plugin: p: T");
}

#[test]
fn batch_commit_message_format() {
    assert_eq!(
        batch_commit_message("audit", &["a".into(), "b".into()]),
        "balls: batch audit\n\nparticipants: a, b"
    );
}
