//! §9 `create` front-door dispatch tests (§10/§15): `--parent` containment,
//! `--needs B[:OP]`, `--blocks OP`/`--blocks ID:OP`, the live-target refusal
//! (bl-6b8c — on a throwaway git store, the one authoring read that walks
//! history), the unknown-op and missing-parent errors, and the one-positional
//! guard. Shares the parent module's `flags`/`write`/`new_id`/`TASK` fixtures
//! via [`super`].

use super::*;
use crate::reads::test_support::{git_store, task};

#[test]
fn create_authors_a_ball_with_its_front_door_structure() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    write(dir, "bl-9", TASK); // an edge target must be live (bl-6b8c)
    let mut f = flags();
    f.positionals = vec!["New thing".into()];
    f.parent = Some("bl-1000".into());
    f.needs = vec!["bl-9".into()];
    f.priority = Some(2);
    f.tags = vec!["x".into()];
    let (base, before) = base_change(Verb::Create, dir, &f, 42).unwrap();
    assert!(before.is_none()); // create has no op-start ball
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000", "bl-9"]); // both pre-existing balls (bl-8c74)
    let t = read_task(dir, &id).unwrap();
    assert_eq!(t.title, "New thing");
    assert_eq!(t.created, 42);
    assert_eq!(t.priority, Some(2));
    assert_eq!(t.tags, ["x"]);
    assert_eq!(t.parent.as_deref(), Some("bl-1000"));
    // --needs is a claim-blocker on the child itself (default op = claim, §10).
    assert_eq!(t.blockers, vec![Blocker { id: "bl-9".into(), on: On::Claim }]);
    // --parent is CONTAINMENT only — it mints NO reciprocal blocker (§10/§15).
    assert!(read_task(dir, "bl-1000").unwrap().blockers.is_empty());
}

#[test]
fn create_blocks_a_bare_op_gates_the_parent() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["Gate".into()];
    f.parent = Some("bl-1000".into());
    f.blocks = vec!["close".into()]; // bare OP → the new ball close-blocks its parent
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
    assert_eq!(read_task(dir, "bl-1000").unwrap().blockers, vec![Blocker { id, on: On::Close }]);
}

#[test]
fn create_blocks_an_id_op_gates_a_non_parent() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-other", TASK);
    let mut f = flags();
    f.positionals = vec!["Edge".into()];
    f.blocks = vec!["bl-other:update".into()]; // ID:OP gates a non-parent's op
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-other"]);
    assert_eq!(read_task(dir, "bl-other").unwrap().blockers, vec![Blocker { id, on: On::Update }]);
}

#[test]
fn create_needs_with_an_explicit_op() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-dep", TASK);
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.needs = vec!["bl-dep:close".into()];
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-dep"]);
    assert_eq!(read_task(dir, &id).unwrap().blockers, vec![Blocker { id: "bl-dep".into(), on: On::Close }]);
}

#[test]
fn create_blocks_a_bare_op_requires_a_parent() {
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.blocks = vec!["close".into()]; // bare OP, no --parent ⇒ no target
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("needs --parent"));
}

#[test]
fn create_refuses_an_unknown_edge_target() {
    // bl-6b8c: under fixed random ids a never-minted target is always a typo
    // or a hallucination, and what it buys is a silently ungated task — the
    // refusal names the id and that it is unknown.
    let s = git_store();
    s.create("bl-live", &task("Alive", 1), 1);
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.needs = vec!["bl-nope".into()];
    let err = base_change(Verb::Create, s.dir(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("'bl-nope' is not a known id"), "{err}");
}

#[test]
fn create_refuses_a_dead_blocks_target_naming_the_closure() {
    // bl-6b8c: a dead target is an edge born resolved — a blocker that can
    // never block — and the refusal carries the fact the author lacked.
    let s = git_store();
    s.create("bl-dead", &task("Done", 1), 1).retire("bl-dead", "close", 2);
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.blocks = vec!["bl-dead:close".into()];
    let err = base_change(Verb::Create, s.dir(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("'bl-dead' is already closed"), "{err}");
}

#[test]
fn create_rejects_an_unknown_op_token() {
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.needs = vec!["bl-dep:frobnicate".into()];
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("not a known op"));
}

#[test]
fn create_rejects_no_needs() {
    // --no-needs drops an existing edge — a fresh ball has none, so it is update-only.
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.no_needs = vec!["bl-x".into()];
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("only for update"));
}

#[test]
fn create_rejects_each_removal_flag_on_its_own() {
    // The removal guard is a pure disjunction: each --no-* flag must bounce
    // ALONE (an && slipped into the chain would let a lone flag through).
    let solo: &[fn(&mut Flags)] = &[
        |f| f.no_parent = true,
        |f| f.no_priority = true,
        |f| f.no_tags = vec!["x".into()],
    ];
    for (i, set) in solo.iter().enumerate() {
        let mut f = flags();
        f.positionals = vec!["t".into()];
        set(&mut f);
        let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
        assert!(err.to_string().contains("only for update"), "solo removal flag #{i}: {err}");
    }
}

#[test]
fn create_requires_exactly_one_positional() {
    let dir = tempdir().unwrap();
    assert!(base_change(Verb::Create, dir.path(), &flags(), 0).is_err()); // zero
    let mut f = flags();
    f.positionals = vec!["a".into(), "b".into()];
    assert!(base_change(Verb::Create, dir.path(), &f, 0).is_err()); // two
}

#[test]
fn create_subtask_of_sets_parent_and_close_gates_it() {
    // §10 sugar: --subtask-of E ≡ --parent E --blocks close — the intent named
    // by the flag, so the close-gate cannot be silently forgotten.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["Subtask".into()];
    f.subtask_of = Some("bl-1000".into());
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
    assert_eq!(read_task(dir, &id).unwrap().parent.as_deref(), Some("bl-1000"));
    assert_eq!(read_task(dir, "bl-1000").unwrap().blockers, vec![Blocker { id, on: On::Close }]);
}

#[test]
fn create_subtask_of_conflicts_with_parent() {
    // --subtask-of IS a parent spelling — naming both is a conflict, never a
    // silent pick.
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.subtask_of = Some("bl-a".into());
    f.parent = Some("bl-b".into());
    let err = base_change(Verb::Create, tempdir().unwrap().path(), &f, 0).err().unwrap();
    assert!(err.to_string().contains("--subtask-of and --parent conflict"));
}

#[test]
fn create_subtask_of_dedups_an_explicit_close_gate() {
    // --subtask-of E --blocks close would mint the same {child, close} edge
    // twice (the bare OP targets the effective parent = E); it converges to one.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["Subtask".into()];
    f.subtask_of = Some("bl-1000".into());
    f.blocks = vec!["close".into()];
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
    assert_eq!(read_task(dir, "bl-1000").unwrap().blockers, vec![Blocker { id, on: On::Close }]);
}

#[test]
fn create_subtask_of_composes_with_a_distinct_blocks_edge() {
    // The sugar's gate appends alongside (not instead of) an explicit non-close
    // edge — both land on the parent.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["Subtask".into()];
    f.subtask_of = Some("bl-1000".into());
    f.blocks = vec!["update".into()];
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
    let blockers = read_task(dir, "bl-1000").unwrap().blockers;
    assert_eq!(blockers.len(), 2);
    assert!(blockers.contains(&Blocker { id: id.clone(), on: On::Update }));
    assert!(blockers.contains(&Blocker { id, on: On::Close }));
}
