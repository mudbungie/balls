//! Forward-compat passthrough coverage for the top-level `Task.extra`
//! / `Note.extra` flatten targets and the per-plugin `synced_at`
//! map: survival across a load/save cycle and the sorted-key
//! mergeable on-disk format. Nested-type (`Link`, `ArchivedChild`)
//! cases live in `task_io_compat_tests.rs`. Split from
//! `task_io_tests.rs`.

use super::*;
use crate::task::{NewTaskOpts, Task};
use tempfile::TempDir;

fn fresh_task(id: &str) -> Task {
    Task::new(
        NewTaskOpts {
            title: "t".into(),
            ..Default::default()
        },
        id.into(),
    )
}

#[test]
fn unknown_top_level_field_round_trips_through_save() {
    // Forward-compat: a future bl may write top-level fields the
    // current struct doesn't name. They must survive a load/save
    // cycle via the #[serde(flatten)] extra passthrough AND land in
    // the sorted-key walk inside serialize_mergeable so git-merge
    // still sees a clean text format.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-fc01.json");
    let t = fresh_task("bl-fc01");
    t.save(&path).unwrap();

    // Inject an unknown top-level field by hand.
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v.as_object_mut()
        .unwrap()
        .insert("severity".to_string(), serde_json::json!("sev2"));
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let loaded = Task::load(&path).unwrap();
    assert_eq!(
        loaded.extra.get("severity").and_then(|v| v.as_str()),
        Some("sev2"),
        "unknown field must land in `extra` on deserialize",
    );

    // Re-save and verify the file still carries `severity` at top level,
    // in sorted-key order per the mergeable format.
    loaded.save(&path).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(v2["severity"], serde_json::json!("sev2"));

    // And the reloaded task still carries it in `extra`.
    let reload = Task::load(&path).unwrap();
    assert_eq!(
        reload.extra.get("severity").and_then(|v| v.as_str()),
        Some("sev2"),
    );
}

#[test]
fn unknown_note_field_is_preserved_on_load() {
    // Forward-compat on the sidecar: notes are append-only so they
    // are never re-saved, but a newer bl may write a note JSONL line
    // with a field the current Note struct doesn't name. The load
    // path must capture it into `Note.extra` instead of failing the
    // whole task load.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-fc03.json");
    let t = fresh_task("bl-fc03");
    t.save(&path).unwrap();
    let notes_path = notes_path_for(&path);
    std::fs::write(
        &notes_path,
        "{\"ts\":\"2027-01-01T00:00:00Z\",\"author\":\"future\",\
         \"text\":\"hi\",\"attachment_url\":\"https://x/y\"}\n",
    )
    .unwrap();

    let back = Task::load(&path).unwrap();
    assert_eq!(back.notes.len(), 1);
    assert_eq!(
        back.notes[0].extra.get("attachment_url").and_then(|v| v.as_str()),
        Some("https://x/y"),
    );
}

#[test]
fn extra_map_does_not_collide_with_known_fields() {
    // Belt-and-suspenders: a known field assigned via the on-disk
    // layout must not leak into `extra`. This guards against accidentally
    // widening the flatten target.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-fc02.json");
    let t = fresh_task("bl-fc02");
    t.save(&path).unwrap();
    let back = Task::load(&path).unwrap();
    assert!(back.extra.is_empty());
    assert!(!back.extra.contains_key("id"));
    assert!(!back.extra.contains_key("title"));
}

#[test]
fn synced_at_defaults_to_empty_and_is_omitted_on_save() {
    // A fresh task has no synced_at entries; serialize_mergeable
    // should omit the key entirely (skip_serializing_if empty) so the
    // on-disk format stays identical for tasks no plugin has touched.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-5a00.json");
    let t = fresh_task("bl-5a00");
    assert!(t.synced_at.is_empty());
    t.save(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("synced_at"), "empty synced_at must not serialize: {raw}");
    let back = Task::load(&path).unwrap();
    assert!(back.synced_at.is_empty());
}

#[test]
fn synced_at_roundtrips_per_plugin_timestamps() {
    use chrono::{DateTime, Utc};
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-5a01.json");
    let mut t = fresh_task("bl-5a01");
    let ts: DateTime<Utc> = "2026-04-10T12:00:00Z".parse().unwrap();
    t.synced_at.insert("jira".into(), ts);
    t.synced_at.insert("slack".into(), ts);
    t.save(&path).unwrap();
    let back = Task::load(&path).unwrap();
    assert_eq!(back.synced_at.len(), 2);
    assert_eq!(back.synced_at.get("jira"), Some(&ts));
    assert_eq!(back.synced_at.get("slack"), Some(&ts));
}
