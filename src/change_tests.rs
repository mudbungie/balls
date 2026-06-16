//! §9 base-change tests — each verb's authoring (stage) and §5 message
//! (finalize) on a plain temp dir; finalize asserts the `bl-op` trailer via
//! [`crate::message::parse`], proving the lifecycle seam each verb fills.

use super::*;
use crate::message::parse;
use crate::task::{Blocker, On, Task};
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
fn update_clears_optional_fields_with_none() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", RICH);
    let u = Update {
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
fn retire_errors_when_the_ball_is_absent() {
    let d = tempdir().unwrap();
    let r = Retire::close("bl-gone".into(), "t".into(), "me".into());
    assert!(r.stage(d.path()).is_err());
}

#[test]
fn the_m_message_flows_into_the_commit_body_under_the_title_subject() {
    // The subject is ALWAYS the ball title (no override); `-m` is the free body.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let mut o = Occupancy::claim("bl-1".into(), "me".into(), 0);
    o.message = Some("Extra paragraph.".into());
    o.stage(dir).unwrap();
    let msg = o.finalize(dir).unwrap();
    assert!(msg.starts_with("A task"));
    assert!(msg.contains("Extra paragraph."));
}

#[test]
fn update_is_narrated_iff_it_carries_m() {
    // bl-cf93: the engine consults `narrated()` to refuse a no-op seal that
    // would drop the `-m` note; a note-less update may still converge.
    let noted =
        Update { id: "bl-1".into(), actor: "me".into(), now: 1, edits: vec![], message: Some("n".into()) };
    assert!(noted.narrated());
    let plain = Update { id: "bl-1".into(), actor: "me".into(), now: 1, edits: vec![], message: None };
    assert!(!plain.narrated());
}

// The `create` authoring tests share this module's `write`/`TASK` fixtures.
#[path = "change_create_tests.rs"]
mod create;
