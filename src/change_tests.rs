//! §9 base-change tests — each verb's authoring (stage) and §5 message
//! (finalize) on a plain temp dir; finalize asserts the `bl-op` trailer via
//! [`crate::message::parse`], proving the lifecycle seam each verb fills.

use super::*;
use crate::message::parse;
use crate::task::On;
use tempfile::tempdir;

const TASK: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\n+++\nbody\n";
const CLAIMED: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\nclaimant = \"bob\"\n+++\n";
const RICH: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\nparent = \"bl-old\"\n\
priority = 1\ntags = [\"a\"]\n\n[[blockers]]\nid = \"bl-z\"\non = \"claim\"\n+++\nbody\n";

fn write(dir: &Path, id: &str, md: &str) {
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join(format!("{id}.md")), md).unwrap();
}

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
        over: None,
        body: None,
        existing,
    }
}

#[test]
fn create_stages_a_new_ball_from_injected_id_clock_and_fields() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let c = Create {
        parent: Some(("bl-1000".into(), On::Claim)),
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
    // --parent writes the reciprocal claim-blocker on the epic (§10).
    let parent = read_task(dir, "bl-1000").unwrap();
    assert_eq!(parent.blockers, vec![Blocker { id: "bl-aaaa".into(), on: On::Claim }]);
}

#[test]
fn create_with_gates_writes_a_close_reciprocal_on_the_parent() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1000", TASK);
    let c = Create { parent: Some(("bl-1000".into(), On::Close)), ..create("bl-g", vec![]) };
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

#[test]
fn claim_sets_the_claimant_and_bumps_updated() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let o = Occupancy::claim("bl-1".into(), "alice".into(), 1_749_085_200);
    o.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.claimant.as_deref(), Some("alice"));
    assert_eq!(t.updated, 1_749_085_200);
    let md = parse(&o.finalize(dir).unwrap()).unwrap();
    assert_eq!(md["bl-op"], ["claim"]);
    assert_eq!(md["bl-id"], ["bl-1"]);
}

#[test]
fn claim_refuses_an_already_claimed_ball() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", CLAIMED);
    let o = Occupancy::claim("bl-1".into(), "alice".into(), 0);
    let err = o.stage(dir).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert!(err.to_string().contains("already claimed by bob"));
}

#[test]
fn unclaim_clears_the_claimant() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", CLAIMED);
    let o = Occupancy::unclaim("bl-1".into(), "alice".into(), 22);
    o.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert!(t.claimant.is_none());
    assert_eq!(t.updated, 22);
    let md = parse(&o.finalize(dir).unwrap()).unwrap();
    assert_eq!(md["bl-op"], ["unclaim"]);
}

#[test]
fn update_applies_every_field_edit_and_bumps_updated() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", RICH);
    let u = Update {
        id: "bl-1".into(),
        actor: "me".into(),
        now: 99,
        over: None,
        body: None,
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
fn update_clears_optional_fields_with_none() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", RICH);
    let u = Update {
        id: "bl-1".into(),
        actor: "me".into(),
        now: 99,
        over: None,
        body: None,
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
        id: "bl-1".into(),
        actor: "me".into(),
        now: 99,
        over: None,
        body: None,
        edits: vec![FieldEdit::Title("Renamed".into())],
    };
    u.stage(dir).unwrap();
    let msg = u.finalize(dir).unwrap();
    assert!(msg.starts_with("Renamed"));
    assert_eq!(parse(&msg).unwrap()["bl-op"], ["update"]);
}

#[test]
fn close_removes_the_file_and_emits_a_close_message() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let r = Retire::close("bl-1".into(), "A task".into(), "me".into());
    r.stage(dir).unwrap();
    assert!(!task_path(dir, "bl-1").exists());
    let msg = r.finalize(dir).unwrap();
    assert!(msg.starts_with("A task"));
    let md = parse(&msg).unwrap();
    assert_eq!(md["bl-op"], ["close"]);
    assert_eq!(md["bl-id"], ["bl-1"]);
}

#[test]
fn drop_removes_the_file_and_emits_a_drop_message() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let r = Retire::drop("bl-1".into(), "A task".into(), "me".into());
    r.stage(dir).unwrap();
    assert_eq!(parse(&r.finalize(dir).unwrap()).unwrap()["bl-op"], ["drop"]);
}

#[test]
fn retire_errors_when_the_ball_is_absent() {
    let d = tempdir().unwrap();
    let r = Retire::close("bl-gone".into(), "t".into(), "me".into());
    assert!(r.stage(d.path()).is_err());
}

#[test]
fn an_override_subject_and_body_flow_into_the_message() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let mut o = Occupancy::claim("bl-1".into(), "me".into(), 0);
    o.over = Some("Custom subject".into());
    o.body = Some("Extra paragraph.".into());
    o.stage(dir).unwrap();
    let msg = o.finalize(dir).unwrap();
    assert!(msg.starts_with("Custom subject"));
    assert!(msg.contains("Extra paragraph."));
}
