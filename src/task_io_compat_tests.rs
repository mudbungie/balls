//! Forward-compat round-trip coverage for nested types embedded in
//! the task JSON written to the state branch (`Link` inside
//! `task.links`, `ArchivedChild` inside `task.closed_children`, plus
//! end-to-end version-mismatch). The top-level `Task.extra` and
//! `Note.extra` cases live in `task_io_tests.rs`; this file is the
//! sibling for the catch-alls that were added when state-branch
//! commits started propagating across machines (bl-2148).

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
fn nested_unknown_field_in_link_round_trips_through_save() {
    // A future bl may attach metadata to a Link entry inside
    // `task.links`. The catch-all on Link must round-trip those
    // fields through save/load — without this, an older bl loading
    // the task and saving it back would silently strip link metadata.
    use crate::task::{Link, LinkType};
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-fc04.json");
    let mut t = fresh_task("bl-fc04");
    t.links.push(Link {
        link_type: LinkType::Gates,
        target: "bl-fc04g".into(),
        extra: std::collections::BTreeMap::new(),
    });
    t.save(&path).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let link = v["links"][0].as_object_mut().unwrap();
    link.insert("added_by".into(), serde_json::json!("alice"));
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let loaded = Task::load(&path).unwrap();
    assert_eq!(
        loaded.links[0].extra.get("added_by").and_then(|v| v.as_str()),
        Some("alice"),
    );

    loaded.save(&path).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(v2["links"][0]["added_by"], serde_json::json!("alice"));
}

#[test]
fn nested_unknown_field_in_archived_child_round_trips_through_save() {
    // Same exposure as links: closed_children entries are written
    // into the parent task on the state branch, so a future bl
    // adding fields there must survive a load+save cycle by an
    // older bl.
    use crate::task::ArchivedChild;
    use chrono::Utc;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-fc05.json");
    let mut t = fresh_task("bl-fc05");
    t.closed_children.push(ArchivedChild {
        id: "bl-fc05c".into(),
        title: "kid".into(),
        closed_at: Utc::now(),
        extra: std::collections::BTreeMap::new(),
    });
    t.save(&path).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let child = v["closed_children"][0].as_object_mut().unwrap();
    child.insert("delivered_in".into(), serde_json::json!("deadbeef"));
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let loaded = Task::load(&path).unwrap();
    assert_eq!(
        loaded.closed_children[0]
            .extra
            .get("delivered_in")
            .and_then(|v| v.as_str()),
        Some("deadbeef"),
    );

    loaded.save(&path).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(
        v2["closed_children"][0]["delivered_in"],
        serde_json::json!("deadbeef"),
    );
}

#[test]
fn cross_version_full_load_save_load_preserves_nested_extras() {
    // End-to-end: a "newer bl" writes a task file with extras at the
    // top level, on a link, and on an archived child. The "older bl"
    // (this binary) loads it, mutates an unrelated field, saves, and
    // reloads — every extra must still be there. This is the
    // user-facing guarantee the catch-alls exist to provide.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bl-fc06.json");
    let future_json = r#"{
      "id": "bl-fc06",
      "title": "from the future",
      "type": "task",
      "priority": 2,
      "status": "open",
      "parent": null,
      "depends_on": [],
      "description": "",
      "created_at": "2030-01-01T00:00:00Z",
      "updated_at": "2030-01-01T00:00:00Z",
      "closed_at": null,
      "claimed_by": null,
      "branch": null,
      "tags": [],
      "links": [
        {"link_type":"gates","target":"bl-fc06g","added_by":"future-alice"}
      ],
      "closed_children": [
        {"id":"bl-fc06c","title":"kid","closed_at":"2030-01-02T00:00:00Z","delivered_in":"f00d"}
      ],
      "external": {},
      "delivered_in": null,
      "severity": "sev2",
      "future_top_level": {"nested": [1, 2, 3]}
    }"#;
    std::fs::write(&path, future_json).unwrap();
    std::fs::write(dir.path().join("bl-fc06.notes.jsonl"), "").unwrap();

    let mut loaded = Task::load(&path).unwrap();
    assert_eq!(loaded.extra.get("severity").and_then(|v| v.as_str()), Some("sev2"));
    assert_eq!(
        loaded.links[0].extra.get("added_by").and_then(|v| v.as_str()),
        Some("future-alice"),
    );
    assert_eq!(
        loaded.closed_children[0].extra.get("delivered_in").and_then(|v| v.as_str()),
        Some("f00d"),
    );
    loaded.title = "edited".into();
    loaded.save(&path).unwrap();

    let reloaded = Task::load(&path).unwrap();
    assert_eq!(reloaded.title, "edited");
    assert_eq!(reloaded.extra.get("severity").and_then(|v| v.as_str()), Some("sev2"));
    assert_eq!(
        reloaded.extra.get("future_top_level"),
        Some(&serde_json::json!({"nested": [1, 2, 3]})),
    );
    assert_eq!(
        reloaded.links[0].extra.get("added_by").and_then(|v| v.as_str()),
        Some("future-alice"),
    );
    assert_eq!(
        reloaded.closed_children[0].extra.get("delivered_in").and_then(|v| v.as_str()),
        Some("f00d"),
    );
}
