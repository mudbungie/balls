//! §9 `update` authoring tests — the [`Update`] base-change ([`crate::change::field`]):
//! every [`FieldEdit`] applied and `updated` bumped, the whole-buffer `--edit`
//! replace (created preserved, updated restamped), the on=update blocker refusal,
//! the None-clears, the retitled finalize subject, and `narrated()`. Shares the
//! parent module's `write`/`TASK`/`RICH` fixtures via [`super`].

use super::*;

#[test]
fn update_applies_every_field_edit_and_bumps_updated() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", RICH);
    let u = Update {
        verb: Verb::Update,
        id: "bl-1".into(),
        actor: "me".into(),
        now: 99,
        message: None,
        edits: vec![
            FieldEdit::Title("Renamed".into()),
            FieldEdit::Body("new body\n".into()),
            FieldEdit::Parent(Some("bl-p".into())),
            FieldEdit::Priority(Some(3)),
            FieldEdit::AddTag("a".into()),
            FieldEdit::AddTag("b".into()),
            FieldEdit::RemoveTag("a".into()),
            FieldEdit::AddBlocker(Blocker { id: "bl-x".into(), on: On::Close }),
            FieldEdit::AddBlocker(Blocker { id: "bl-x".into(), on: On::Close }),
            FieldEdit::RemoveBlocker("bl-z".into()),
            FieldEdit::SetExtra("state".into(), "doing".into()),
            FieldEdit::SetExtra("foo".into(), "bar".into()),
            FieldEdit::RemoveExtra("foo".into()),
        ],
    };
    u.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.title, "Renamed");
    assert_eq!(t.body, "new body\n");
    assert_eq!(t.parent.as_deref(), Some("bl-p"));
    assert_eq!(t.priority, Some(3));
    assert_eq!(t.tags, ["b"]);
    assert_eq!(t.blockers, vec![Blocker { id: "bl-x".into(), on: On::Close }]);
    assert_eq!(t.updated, 99);
    assert_eq!(
        t.extra.get("state").and_then(toml::Value::as_str),
        Some("doing")
    );
    assert!(!t.extra.contains_key("foo"));
}

#[test]
fn update_replace_overwrites_the_whole_ball_but_preserves_created() {
    // The `--edit` whole-buffer edit: every field comes from the buffer, except
    // `created` (history, not hand-editable) and `updated` (seal-restamped).
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", RICH);
    let original_created = read_task(dir, "bl-1").unwrap().created;
    let buffer = Task { title: "Hand-edited".into(), created: 999, updated: 999, body: "rewritten\n".into(), ..Task::default() };
    let u = Update {
        verb: Verb::Update,
        id: "bl-1".into(),
        actor: "me".into(),
        now: 7,
        message: None,
        edits: vec![FieldEdit::Replace(Box::new(buffer))],
    };
    u.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.title, "Hand-edited");
    assert_eq!(t.body, "rewritten\n");
    assert_eq!(t.created, original_created, "created is preserved");
    assert_eq!(t.updated, 7, "updated is seal-restamped, never the hand-typed 999");
    // RICH's parent/priority/tags/blockers are gone — the buffer replaced them.
    assert!(t.parent.is_none() && t.priority.is_none() && t.tags.is_empty() && t.blockers.is_empty());
}

#[test]
fn update_is_refused_while_an_on_update_blocker_is_open() {
    // A third `on` (neither claim nor close) is enforced by core (§10/§15): the
    // update op is staged behind enforce::gate, so an open on=update edge blocks.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-dep", TASK); // present ⇒ unresolved
    write(
        dir,
        "bl-1",
        "+++\ntitle = \"A\"\ncreated = 0\nupdated = 0\n\n[[blockers]]\nid = \"bl-dep\"\non = \"update\"\n+++\n",
    );
    let u = Update {
        verb: Verb::Update,
        id: "bl-1".into(),
        actor: "me".into(),
        now: 9,
        message: None,
        edits: vec![FieldEdit::Title("Renamed".into())],
    };
    let err = u.stage(dir).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "update: bl-1 blocked by unresolved bl-dep");
}

#[test]
fn update_refuses_a_needs_edge_that_closes_a_deadlock() {
    // bl-54fe defect 2: the parent already close-gates on the gate; adding the
    // "obvious fix" (gate --needs parent) post-hoc is refused at stage, naming
    // the loop — instead of springing at `bl close` with the work already done.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(
        dir,
        "bl-work",
        "+++\ntitle = \"work\"\ncreated = 0\nupdated = 0\n\n[[blockers]]\nid = \"bl-gate\"\non = \"close\"\n+++\n",
    );
    write(dir, "bl-gate", TASK);
    let u = Update {
        verb: Verb::Update,
        id: "bl-gate".into(),
        actor: "me".into(),
        now: 9,
        message: None,
        edits: vec![FieldEdit::AddBlocker(Blocker { id: "bl-work".into(), on: On::Claim })],
    };
    let err = u.stage(dir).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert!(err.to_string().contains("bl-gate -claim-> bl-work -close-> bl-gate"), "{err}");
}

#[test]
fn update_clears_optional_fields_with_none() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", RICH);
    let u = Update {
        verb: Verb::Update,
        id: "bl-1".into(),
        actor: "me".into(),
        now: 99,
        message: None,
        edits: vec![FieldEdit::Parent(None), FieldEdit::Priority(None)],
    };
    u.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert!(t.parent.is_none());
    assert!(t.priority.is_none());
}

#[test]
fn update_finalizes_with_the_retitled_subject() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let u = Update {
        verb: Verb::Update,
        id: "bl-1".into(),
        actor: "me".into(),
        now: 99,
        message: None,
        edits: vec![FieldEdit::Title("Renamed".into())],
    };
    u.stage(dir).unwrap();
    let msg = u.finalize(dir).unwrap();
    assert!(msg.starts_with("Renamed"));
    assert_eq!(parse(&msg).unwrap()["bl-op"], ["update"]);
}

#[test]
fn update_is_narrated_iff_it_carries_m() {
    // bl-cf93: the engine consults `narrated()` to refuse a no-op seal that
    // would drop the `-m` note; a note-less update may still converge.
    let noted =
        Update { verb: Verb::Update, id: "bl-1".into(), actor: "me".into(), now: 1, edits: vec![], message: Some("n".into()) };
    assert!(noted.narrated());
    let plain = Update { verb: Verb::Update, id: "bl-1".into(), actor: "me".into(), now: 1, edits: vec![], message: None };
    assert!(!plain.narrated());
}
