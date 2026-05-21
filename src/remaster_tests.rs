use super::*;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn next_free_id_returns_first_when_free() {
    let ts = chrono::Utc::now();
    let got = next_free_id("hello", ts, 4, |_| false).unwrap();
    assert_eq!(got, Task::generate_id("hello", ts, 4));
}

#[test]
fn next_free_id_bumps_past_collision() {
    let ts = chrono::Utc::now();
    let first = Task::generate_id("hello", ts, 4);
    let got = next_free_id("hello", ts, 4, |id| id == first).unwrap();
    assert_ne!(got, first);
}

#[test]
fn next_free_id_exhausts() {
    let err = next_free_id("x", chrono::Utc::now(), 4, |_| true).unwrap_err();
    match err {
        BallError::Other(s) => assert!(s.contains("1000 tries")),
        other => panic!("expected Other, got {other:?}"),
    }
}

#[test]
fn fresh_id_rejects_bad_json() {
    let used = BTreeSet::new();
    let err = fresh_id("not json", 4, &used).unwrap_err();
    assert!(matches!(err, BallError::InvalidTask(_)));
}

#[test]
fn fresh_id_from_valid_task() {
    let json = serde_json::to_string(&json!({
        "id": "bl-dead", "title": "t", "type": "task", "priority": 3,
        "status": "open", "parent": null, "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z", "closed_at": null,
        "claimed_by": null, "branch": null
    }))
    .unwrap();
    let used = BTreeSet::new();
    assert!(fresh_id(&json, 4, &used).unwrap().starts_with("bl-"));
}

#[test]
fn read_local_tasks_missing_dir_is_empty() {
    let dir = TempDir::new().unwrap();
    let got = read_local_tasks(&dir.path().join("nope")).unwrap();
    assert!(got.is_empty());
}

#[test]
fn read_local_tasks_skips_scaffolding() {
    let dir = TempDir::new().unwrap();
    let t = dir.path();
    std::fs::write(t.join("bl-aaaa.json"), "{}").unwrap();
    std::fs::write(t.join("bl-aaaa.notes.jsonl"), "x\n").unwrap();
    std::fs::write(t.join(".gitattributes"), "").unwrap();
    std::fs::write(t.join(".gitkeep"), "").unwrap();
    let got = read_local_tasks(t).unwrap();
    assert_eq!(got.keys().collect::<Vec<_>>(), vec!["bl-aaaa"]);
    assert_eq!(got["bl-aaaa"].notes, "x\n");
}

#[test]
fn json_rel_is_under_tasks_dir() {
    assert_eq!(json_rel("bl-abcd"), ".balls/tasks/bl-abcd.json");
}

#[test]
fn write_task_rejects_non_json() {
    let dir = TempDir::new().unwrap();
    let lt = LocalTask {
        json: "not json".into(),
        notes: String::new(),
    };
    let err = write_task(dir.path(), "bl-x", &lt, &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, BallError::InvalidTask(_)));
}

#[test]
fn write_task_rejects_valid_json_that_is_not_a_task() {
    let dir = TempDir::new().unwrap();
    let lt = LocalTask {
        json: "{}".into(),
        notes: String::new(),
    };
    let err = write_task(dir.path(), "bl-x", &lt, &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, BallError::InvalidTask(_)));
}

#[test]
fn write_task_replays_with_notes() {
    let dir = TempDir::new().unwrap();
    let json = serde_json::to_string(&json!({
        "id": "bl-aaaa", "title": "t", "type": "task", "priority": 3,
        "status": "open", "parent": null, "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z", "closed_at": null,
        "claimed_by": null, "branch": null
    }))
    .unwrap();
    let lt = LocalTask {
        json,
        notes: "{\"x\":1}\n".into(),
    };
    write_task(dir.path(), "bl-aaaa", &lt, &BTreeMap::new()).unwrap();
    assert!(dir.path().join("bl-aaaa.json").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("bl-aaaa.notes.jsonl")).unwrap(),
        "{\"x\":1}\n"
    );
}

fn rn(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

#[test]
fn remap_refs_non_object_is_noop() {
    let mut v = json!(42);
    remap_refs(&mut v, "bl-new", &rn(&[]));
    assert_eq!(v, json!(42));
}

#[test]
fn remap_refs_rewrites_id_and_all_reference_shapes() {
    let rename = rn(&[("bl-old", "bl-new"), ("bl-dep", "bl-dep2")]);
    let mut v = json!({
        "id": "bl-old",
        "parent": "bl-old",
        "depends_on": ["bl-dep", "bl-keep", 7],
        "links": [{"target": "bl-old"}, {"note": "x"}, {"target": 9}],
        "closed_children": [{"id": "bl-dep"}],
    });
    remap_refs(&mut v, "bl-new", &rename);
    assert_eq!(v["id"], json!("bl-new"));
    assert_eq!(v["parent"], json!("bl-new"));
    assert_eq!(v["depends_on"], json!(["bl-dep2", "bl-keep", 7]));
    assert_eq!(v["links"][0]["target"], json!("bl-new"));
    assert_eq!(v["links"][2]["target"], json!(9));
    assert_eq!(v["closed_children"][0]["id"], json!("bl-dep2"));
}

#[test]
fn remap_refs_handles_absent_and_nonstring_parent() {
    let mut absent = json!({"id": "bl-x"});
    remap_refs(&mut absent, "bl-y", &rn(&[]));
    assert_eq!(absent["id"], json!("bl-y"));
    assert!(absent.get("parent").is_none());

    let mut nonstr = json!({"id": "bl-x", "parent": null});
    remap_refs(&mut nonstr, "bl-y", &rn(&[]));
    assert_eq!(nonstr["parent"], json!(null));
}

#[test]
fn remap_array_ignores_missing_and_non_array() {
    let map = |s: &str| s.to_string();
    remap_array(None, &map);
    let mut not_arr = json!("scalar");
    remap_array(Some(&mut not_arr), &map);
    assert_eq!(not_arr, json!("scalar"));
}

#[test]
fn remap_field_array_ignores_missing_non_array_and_non_object_elems() {
    let map = |s: &str| s.to_string();
    remap_field_array(None, "target", &map);
    let mut not_arr = json!(1);
    remap_field_array(Some(&mut not_arr), "target", &map);
    assert_eq!(not_arr, json!(1));
    let mut arr = json!(["bare", {"other": 1}]);
    remap_field_array(Some(&mut arr), "target", &map);
    assert_eq!(arr, json!(["bare", {"other": 1}]));
}

#[test]
fn scrub_legacy_canonical_noop_when_config_absent() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    // No config.json present — nothing to scrub, returns Ok.
    scrub_legacy_canonical(dir.path()).unwrap();
}

#[test]
fn scrub_legacy_canonical_noop_when_config_is_symlink() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    let real = dir.path().join(".balls/real.json");
    std::fs::write(&real, "{}").unwrap();
    std::os::unix::fs::symlink(&real, dir.path().join(".balls/config.json")).unwrap();
    // A symlinked canonical is the migrated shape — skip it.
    scrub_legacy_canonical(dir.path()).unwrap();
}

#[test]
fn scrub_legacy_canonical_noop_when_config_unreadable() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    std::fs::write(dir.path().join(".balls/config.json"), "not json").unwrap();
    // Garbage config: scrub can't parse it, returns Ok without touching it.
    scrub_legacy_canonical(dir.path()).unwrap();
}

#[test]
fn scrub_legacy_canonical_clears_master_url_and_state_remote() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    let cfg_path = dir.path().join(".balls/config.json");
    let cfg = crate::config::Config {
        master_url: Some("hub".into()),
        state_remote: Some("upstream".into()),
        ..crate::config::Config::default()
    };
    cfg.save(&cfg_path).unwrap();
    scrub_legacy_canonical(dir.path()).unwrap();
    let after = crate::config::Config::load(&cfg_path).unwrap();
    assert_eq!(after.master_url, None);
    assert_eq!(after.state_remote, None);
}

#[test]
fn scrub_legacy_canonical_noop_when_fields_already_clear() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    let cfg_path = dir.path().join(".balls/config.json");
    crate::config::Config::default().save(&cfg_path).unwrap();
    scrub_legacy_canonical(dir.path()).unwrap();
}
