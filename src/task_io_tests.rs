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
fn serialize_keys_are_sorted_and_one_per_line() {
    let t = fresh_task("bl-abcd");
    let s = serialize_mergeable(&t).unwrap();

    // Collect each top-level key in the order it appears.
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.first().unwrap(), &"{");
    assert_eq!(lines.last().unwrap(), &"}");

    let mut keys: Vec<String> = Vec::new();
    for line in &lines[1..lines.len() - 1] {
        let trimmed = line.trim_start();
        let end = trimmed.find('"').unwrap();
        let rest = &trimmed[end + 1..];
        let close = rest.find('"').unwrap();
        keys.push(rest[..close].to_string());
    }
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "keys must be serialized in sorted order");
    assert!(!keys.contains(&"notes".to_string()), "notes must not live in task.json");
}

#[test]
fn serialize_ends_with_newline() {
    let t = fresh_task("bl-ef01");
    let s = serialize_mergeable(&t).unwrap();
    assert!(s.ends_with('\n'));
}

#[test]
fn serialize_is_deterministic_for_same_state() {
    let t = fresh_task("bl-1234");
    let a = serialize_mergeable(&t).unwrap();
    let b = serialize_mergeable(&t).unwrap();
    assert_eq!(a, b);
}

#[test]
fn save_then_load_roundtrips_without_notes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-aaaa.json");
    let t = fresh_task("bl-aaaa");
    t.save(&path).unwrap();
    let back = Task::load(&path).unwrap();
    assert_eq!(back.id, "bl-aaaa");
    assert_eq!(back.notes.len(), 0);
}

#[test]
fn append_note_to_persists_and_load_reads_it_back() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-bbbb.json");
    let t = fresh_task("bl-bbbb");
    t.save(&path).unwrap();

    append_note_to(&path, "alice", "first").unwrap();
    append_note_to(&path, "bob", "second").unwrap();

    let back = Task::load(&path).unwrap();
    assert_eq!(back.notes.len(), 2);
    assert_eq!(back.notes[0].author, "alice");
    assert_eq!(back.notes[0].text, "first");
    assert_eq!(back.notes[1].author, "bob");
    assert_eq!(back.notes[1].text, "second");
}

#[test]
fn append_note_to_uses_sibling_file_path() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-cccc.json");
    let t = fresh_task("bl-cccc");
    t.save(&path).unwrap();
    append_note_to(&path, "x", "y").unwrap();
    let notes_file = dir.path().join("bl-cccc.notes.jsonl");
    assert!(notes_file.exists());
    let content = std::fs::read_to_string(&notes_file).unwrap();
    assert!(content.ends_with('\n'));
    assert_eq!(content.lines().count(), 1);
}

#[test]
fn save_does_not_touch_notes_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-dddd.json");
    let t = fresh_task("bl-dddd");
    t.save(&path).unwrap();
    append_note_to(&path, "a", "1").unwrap();
    // Resave: must not wipe the notes file
    t.save(&path).unwrap();
    let back = Task::load(&path).unwrap();
    assert_eq!(back.notes.len(), 1);
}

#[test]
fn load_migrates_legacy_in_json_notes() {
    // A legacy file where notes live inside task.json (pre-split format).
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-eeee.json");
    let legacy = r#"{
  "id": "bl-eeee",
  "title": "legacy",
  "type": "task",
  "priority": 3,
  "status": "open",
  "parent": null,
  "depends_on": [],
  "description": "",
  "created_at": "2026-01-01T00:00:00Z",
  "updated_at": "2026-01-01T00:00:00Z",
  "closed_at": null,
  "claimed_by": null,
  "branch": null,
  "tags": [],
  "notes": [{"ts":"2026-01-01T00:00:00Z","author":"legacy","text":"old"}],
  "links": [],
  "closed_children": [],
  "external": {}
}"#;
    std::fs::write(&path, legacy).unwrap();
    let back = Task::load(&path).unwrap();
    assert_eq!(back.notes.len(), 1);
    assert_eq!(back.notes[0].author, "legacy");
}

#[test]
fn delete_notes_file_removes_sibling() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-ffff.json");
    let t = fresh_task("bl-ffff");
    t.save(&path).unwrap();
    append_note_to(&path, "a", "b").unwrap();
    let notes_file = notes_path_for(&path);
    assert!(notes_file.exists());
    delete_notes_file(&path).unwrap();
    assert!(!notes_file.exists());
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

#[test]
fn load_notes_file_skips_blank_lines() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-b1ff.json");
    let t = fresh_task("bl-b1ff");
    t.save(&path).unwrap();
    let notes_path = notes_path_for(&path);
    // Hand-write a notes file with a blank line between real entries;
    // loader must skip it instead of failing JSON-parse on "".
    std::fs::write(
        &notes_path,
        "{\"ts\":\"2026-01-01T00:00:00Z\",\"author\":\"a\",\"text\":\"one\"}\n\
         \n\
         {\"ts\":\"2026-01-01T00:00:01Z\",\"author\":\"a\",\"text\":\"two\"}\n",
    )
    .unwrap();
    let back = Task::load(&path).unwrap();
    assert_eq!(back.notes.len(), 2);
}
