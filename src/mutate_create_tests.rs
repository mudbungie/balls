//! §9 `create` front-door dispatch tests (§10/§15): `--parent` containment,
//! `--needs B[:OP]`, `--blocks OP`/`--blocks ID:OP`, the unknown-op and
//! missing-parent errors, and the one-positional guard. Shares the parent
//! module's `flags`/`write`/`new_id`/`TASK` fixtures via [`super`].

use super::*;

#[test]
fn create_authors_a_ball_with_its_front_door_structure() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let mut f = flags();
    f.positionals = vec!["New thing".into()];
    f.parent = Some("bl-1000".into());
    f.needs = vec!["bl-9".into()];
    f.priority = Some(2);
    f.tags = vec!["x".into()];
    let (base, before) = base_change(Verb::Create, dir, &f, 42).unwrap();
    assert!(before.is_none()); // create has no op-start ball
    base.stage(dir).unwrap();
    let id = new_id(dir, &["bl-1000"]);
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
    let mut f = flags();
    f.positionals = vec!["t".into()];
    f.needs = vec!["bl-dep:close".into()];
    let (base, _) = base_change(Verb::Create, dir, &f, 0).unwrap();
    base.stage(dir).unwrap();
    let id = new_id(dir, &[]);
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
