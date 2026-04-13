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
    assert!(s.ends_with("\n"));
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
    assert!(content.ends_with("\n"));
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
