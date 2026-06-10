//! §9 `create` base-change tests — the authoring of a fresh ball (id/clock/
//! fields, the §10/§15 front door) and the finalize id-discovery path (single
//! new file, `create/pre` reassignment, the validation errors). Shares the parent
//! module's `write`/`TASK` fixtures via [`super`].

use super::*;

/// A bare [`Create`] for `id` over an `existing` id set, every front-door field
/// empty — tests override only what they exercise.
fn create(id: &str, existing: Vec<String>) -> Create {
    Create {
        id: id.into(),
        actor: "me".into(),
        now: 0,
        title: "Created title".into(),
        parent: None,
        priority: None,
        tags: vec![],
        blockers: vec![],
        blocks: vec![],
        body: None,
        message: None,
        existing,
    }
}

#[test]
fn create_stages_a_new_ball_from_injected_id_clock_and_fields() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let c = Create {
        parent: Some("bl-1000".into()),
        priority: Some(2),
        tags: vec!["x".into()],
        blockers: vec![Blocker { id: "bl-9".into(), on: On::Claim }],
        now: 1_749_081_600,
        title: "New thing".into(),
        ..create("bl-aaaa", vec![])
    };
    c.stage(dir).unwrap();
    let t = read_task(dir, "bl-aaaa").unwrap();
    assert_eq!(t.title, "New thing");
    assert_eq!(t.created, 1_749_081_600);
    assert_eq!(t.updated, 1_749_081_600);
    assert_eq!(t.parent.as_deref(), Some("bl-1000"));
    assert_eq!(t.priority, Some(2));
    assert_eq!(t.tags, ["x"]);
    assert_eq!(t.blockers, vec![Blocker { id: "bl-9".into(), on: On::Claim }]);
    assert!(t.claimant.is_none());
    // --parent is CONTAINMENT only (§10/§15) — it mints NO reciprocal blocker.
    let parent = read_task(dir, "bl-1000").unwrap();
    assert!(parent.blockers.is_empty());
}

#[test]
fn create_writes_the_ball_body_from_the_body_flag() {
    // `--body` sets the ball's markdown body at create (no longer just a commit
    // note); an absent `--body` leaves it empty.
    let d = tempdir().unwrap();
    let dir = d.path();
    let c = Create { body: Some("## Plan\nfirst step\n".into()), ..create("bl-bbbb", vec![]) };
    c.stage(dir).unwrap();
    assert_eq!(read_task(dir, "bl-bbbb").unwrap().body, "## Plan\nfirst step\n");
    let bare = create("bl-cccc", vec![]);
    bare.stage(dir).unwrap();
    assert_eq!(read_task(dir, "bl-cccc").unwrap().body, "");
}

#[test]
fn create_blocks_writes_the_named_reciprocal_on_the_target() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    // `--blocks close` (sugared `--gates`): the new ball close-blocks bl-1000.
    let c = Create { blocks: vec![("bl-1000".into(), On::Close)], ..create("bl-g", vec![]) };
    c.stage(dir).unwrap();
    let parent = read_task(dir, "bl-1000").unwrap();
    assert_eq!(parent.blockers, vec![Blocker { id: "bl-g".into(), on: On::Close }]);
}

#[test]
fn create_finalizes_the_new_id_into_a_create_message() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-old", TASK);
    let c = create("bl-new", vec!["bl-old".into()]);
    c.stage(dir).unwrap();
    let msg = c.finalize(dir).unwrap();
    assert!(msg.starts_with("Created title"));
    let md = parse(&msg).unwrap();
    assert_eq!(md["bl-op"], ["create"]);
    assert_eq!(md["bl-id"], ["bl-new"]);
    assert_eq!(md["bl-actor"], ["me"]);
}

#[test]
fn create_finds_the_id_after_a_pre_plugin_renamed_it() {
    let d = tempdir().unwrap();
    let dir = d.path();
    let c = create("bl-new", vec![]);
    c.stage(dir).unwrap();
    fs::rename(dir.join("tasks/bl-new.md"), dir.join("tasks/bl-xyz.md")).unwrap();
    let md = parse(&c.finalize(dir).unwrap()).unwrap();
    assert_eq!(md["bl-id"], ["bl-xyz"]);
}

#[test]
fn create_finalize_names_a_pre_reassignment_that_collided_with_a_live_id() {
    // bl-3ddb: a create.pre plugin `git mv`s the staged file ONTO an existing
    // id — finalize sees zero new ids. "expected exactly one new task file,
    // found 0" was oblique; the staged fingerprint (this op's clock + title)
    // under a pre-existing id names the collision.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-old", TASK);
    let c = create("bl-new", vec!["bl-old".into()]);
    c.stage(dir).unwrap();
    fs::rename(dir.join("tasks/bl-new.md"), dir.join("tasks/bl-old.md")).unwrap();
    let err = c.finalize(dir).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
        err.to_string(),
        "create: a create.pre plugin reassigned the new task to `bl-old`, which already exists — id collision, nothing sealed"
    );
}

#[test]
fn create_finalize_errors_when_no_new_file_was_staged() {
    let d = tempdir().unwrap();
    let err = create("bl-x", vec![]).finalize(d.path()).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("found 0"));
}

#[test]
fn create_finalize_errors_on_more_than_one_new_file() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-a", TASK);
    write(dir, "bl-b", TASK);
    fs::write(dir.join("tasks/notes.txt"), "ignored").unwrap();
    let err = create("bl-a", vec![]).finalize(dir).unwrap_err();
    assert!(err.to_string().contains("found 2"));
}

#[test]
fn create_finalize_rejects_an_invalid_reassigned_id() {
    let d = tempdir().unwrap();
    let dir = d.path();
    let c = create("bl-new", vec![]);
    c.stage(dir).unwrap();
    fs::rename(dir.join("tasks/bl-new.md"), dir.join("tasks/bad.id.md")).unwrap();
    let err = c.finalize(dir).unwrap_err();
    assert!(err.to_string().contains("invalid task id"));
}
