use super::*;
use crate::plugin::{SyncCreate, SyncDelete, SyncReport, SyncUpdate};
use crate::store::Store;
use std::collections::BTreeMap;
use std::path::Path;
use tempfile::TempDir;

fn fresh_store() -> (TempDir, Store) {
    let dir = tempfile::Builder::new()
        .prefix("balls-human-gate-")
        .tempdir()
        .unwrap();
    init_repo(dir.path());
    let store = Store::init(dir.path(), false, None).unwrap();
    (dir, store)
}

fn init_repo(path: &Path) {
    use std::process::Command;
    // Clear inherited GIT_* vars (pre-commit hooks set GIT_DIR, which
    // would otherwise redirect this `git init` away from the temp dir
    // and back to the host repo).
    let env_vars = [
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_WORK_TREE",
        "GIT_PREFIX",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ];
    let run = |args: &[&str]| {
        let mut cmd = Command::new("git");
        cmd.current_dir(path).args(args);
        for v in &env_vars {
            cmd.env_remove(v);
        }
        cmd.output().unwrap();
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "h@example.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["config", "commit.gpgsign", "false"]);
}

fn sample_report() -> SyncReport {
    SyncReport {
        created: vec![SyncCreate {
            title: "from remote".into(),
            task_type: "task".into(),
            priority: 2,
            status: "open".into(),
            description: String::new(),
            tags: Vec::new(),
            external: serde_json::Map::new(),
        }],
        updated: vec![SyncUpdate {
            task_id: "bl-aaaa".into(),
            fields: BTreeMap::new(),
            external: serde_json::Map::new(),
            add_note: None,
        }],
        deleted: vec![SyncDelete {
            task_id: "bl-bbbb".into(),
            reason: "removed upstream".into(),
        }],
    }
}

#[test]
fn stage_writes_file_under_local_pending() {
    let (_dir, store) = fresh_store();
    let id = stage_sync(&store, "jira", &sample_report()).unwrap();
    let path = store
        .local_dir()
        .join("pending-sync")
        .join("sync")
        .join(format!("{id}.json"));
    assert!(path.exists(), "expected staged file at {path:?}");
}

#[test]
fn load_staged_round_trips_report() {
    let (_dir, store) = fresh_store();
    let report = sample_report();
    let id = stage_sync(&store, "jira", &report).unwrap();
    let entry = load_staged(&store, &id).unwrap();
    assert_eq!(entry.data.plugin, "jira");
    assert_eq!(entry.data.event, "sync");
    assert_eq!(entry.data.report.created.len(), 1);
    assert_eq!(entry.data.report.updated[0].task_id, "bl-aaaa");
    assert_eq!(entry.data.report.deleted[0].reason, "removed upstream");
}

#[test]
fn list_staged_returns_all_entries() {
    let (_dir, store) = fresh_store();
    let a = stage_sync(&store, "jira", &sample_report()).unwrap();
    let b = stage_sync(&store, "linear", &sample_report()).unwrap();
    let entries = list_staged(&store).unwrap();
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&a.as_str()));
    assert!(ids.contains(&b.as_str()));
    assert_eq!(entries.len(), 2);
}

#[test]
fn list_staged_returns_empty_when_dir_missing() {
    let (_dir, store) = fresh_store();
    let entries = list_staged(&store).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn discard_removes_the_file_only() {
    let (_dir, store) = fresh_store();
    let id = stage_sync(&store, "jira", &sample_report()).unwrap();
    let other = stage_sync(&store, "linear", &sample_report()).unwrap();
    let (event, plugin) = discard_staged(&store, &id).unwrap();
    assert_eq!(event, "sync");
    assert_eq!(plugin, "jira");
    assert!(load_staged(&store, &id).is_err(), "discarded entry still loadable");
    load_staged(&store, &other).expect("sibling entry still present");
}

#[test]
fn load_staged_missing_id_errors() {
    let (_dir, store) = fresh_store();
    let err = load_staged(&store, "nope").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("nope"), "error should mention id: {msg}");
}

#[test]
fn load_staged_skips_non_directory_entries() {
    let (_dir, store) = fresh_store();
    let id = stage_sync(&store, "jira", &sample_report()).unwrap();
    let root = store.local_dir().join("pending-sync");
    // Stray non-dir entry at the event-dir level: load_staged must
    // skip it instead of treating it as an event subdir.
    std::fs::write(root.join("stray-file"), b"x").unwrap();
    let entry = load_staged(&store, &id).unwrap();
    assert_eq!(entry.id, id);
}

#[test]
fn load_staged_no_root_errors() {
    let (_dir, store) = fresh_store();
    let err = load_staged(&store, "anything").unwrap_err();
    assert!(format!("{err}").contains("anything"));
}

#[test]
fn discard_missing_id_errors() {
    let (_dir, store) = fresh_store();
    let err = discard_staged(&store, "absent").unwrap_err();
    assert!(format!("{err}").contains("absent"));
}

#[test]
fn stage_collisions_bump_and_succeed() {
    let (_dir, store) = fresh_store();
    let fixed = chrono::DateTime::parse_from_rfc3339("2026-04-28T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let a = stage_sync_at(&store, "jira", &sample_report(), fixed).unwrap();
    let b = stage_sync_at(&store, "jira", &sample_report(), fixed).unwrap();
    assert_ne!(a, b, "second staging at identical clock must bump millis");
}

#[test]
fn stage_exhaustion_after_1001_collisions_errors() {
    let (_dir, store) = fresh_store();
    let dir = store.local_dir().join("pending-sync").join("sync");
    std::fs::create_dir_all(&dir).unwrap();
    let start = chrono::DateTime::parse_from_rfc3339("2026-04-28T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut t = start;
    for _ in 0..=1001 {
        let id = make_stage_id("jira", t);
        std::fs::write(dir.join(format!("{id}.json")), b"{}").unwrap();
        t += chrono::Duration::milliseconds(1);
    }
    let err = stage_sync_at(&store, "jira", &sample_report(), start).unwrap_err();
    assert!(
        format!("{err}").contains("unique stage id"),
        "expected exhaustion error, got: {err}"
    );
}

#[test]
fn list_skips_non_directory_and_non_json() {
    let (_dir, store) = fresh_store();
    let id = stage_sync(&store, "jira", &sample_report()).unwrap();
    let root = store.local_dir().join("pending-sync");
    std::fs::write(root.join("not-a-dir"), b"x").unwrap();
    std::fs::write(root.join("sync").join("stray.txt"), b"x").unwrap();
    let entries = list_staged(&store).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, id);
}
