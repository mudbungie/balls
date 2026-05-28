use super::*;
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Stand up a legacy `Store` rooted at `td`, with state-repo seeded
/// via the default address (no origin yet). `tasks` are the local-only
/// task ids to commit on the state branch.
fn legacy_store(td: &Path, tasks: &[(&str, &str)]) -> crate::store::Store {
    crate::git_test_support::init_repo(td);
    let store = crate::store::Store::init(td, false, None).unwrap();
    let tasks_dir = store.state_repo_dir().join(".balls/tasks");
    for (id, title) in tasks {
        let json = serde_json::to_string(&json!({
            "id": id, "title": title, "type": "task", "priority": 3,
            "status": "open", "parent": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "closed_at": null, "claimed_by": null, "branch": null
        })).unwrap();
        std::fs::write(tasks_dir.join(format!("{id}.json")), json).unwrap();
    }
    store.commit_task("local", "balls: seed").unwrap_or(());
    let _ = Command::new("git").current_dir(store.state_repo_dir())
        .args(["add", "-A"]).output();
    let _ = Command::new("git").current_dir(store.state_repo_dir())
        .args(["commit", "-qm", "seed local", "--no-verify"]).output();
    store
}

/// Bare repo with a `balls/tasks` branch carrying the given task
/// payloads (one commit per task). Used as the remaster target.
fn seeded_tracker(td: &Path, branch: &str, tasks: &[(&str, &str)]) -> PathBuf {
    let bare = td.join("target.git");
    Command::new("git")
        .args(["init", "-q", "--bare", "-b", branch])
        .arg(&bare)
        .output()
        .unwrap();
    let seed = td.join("seed");
    std::fs::create_dir_all(&seed).unwrap();
    for args in [
        vec!["init", "-q", "-b", branch],
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        Command::new("git").current_dir(&seed).args(&args).output().unwrap();
    }
    let tasks_dir = seed.join(".balls/tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(tasks_dir.join(".gitkeep"), "").unwrap();
    for (id, title) in tasks {
        let json = serde_json::to_string(&json!({
            "id": id, "title": title, "type": "task", "priority": 3,
            "status": "open", "parent": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "closed_at": null, "claimed_by": null, "branch": null
        })).unwrap();
        std::fs::write(tasks_dir.join(format!("{id}.json")), json).unwrap();
    }
    Command::new("git").current_dir(&seed).args(["add", "-A"]).output().unwrap();
    Command::new("git").current_dir(&seed)
        .args(["commit", "-qm", "seed tracker", "--no-verify"]).output().unwrap();
    Command::new("git").current_dir(&seed)
        .args(["push", "-q", bare.to_str().unwrap(), branch]).output().unwrap();
    bare
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
fn reconcile_seeds_a_fresh_tracker() {
    let td = TempDir::new().unwrap();
    let store = legacy_store(td.path(), &[("bl-aaaa", "local")]);
    // Empty bare tracker — no balls/tasks branch.
    let bare = td.path().join("empty.git");
    Command::new("git")
        .args(["init", "-q", "--bare", "-b", "balls/tasks"])
        .arg(&bare)
        .output()
        .unwrap();
    let outcome = reconcile(&store, bare.to_str().unwrap()).unwrap();
    assert!(matches!(outcome, Reconciled::Seeded), "got: {outcome:?}");
}

#[test]
fn reconcile_clash_renames_then_joins() {
    let td = TempDir::new().unwrap();
    // Local has bl-aaaa with one title; tracker has bl-aaaa with a
    // different title — an id-clash with divergent json.
    let store = legacy_store(td.path(), &[("bl-aaaa", "local-side")]);
    let bare = seeded_tracker(td.path(), "balls/tasks", &[("bl-aaaa", "tracker-side")]);
    let outcome = reconcile(&store, bare.to_str().unwrap()).unwrap();
    let Reconciled::Joined { replayed, renamed } = outcome else {
        panic!("expected Joined, got: {outcome:?}");
    };
    assert_eq!(replayed, 0);
    assert_eq!(renamed, 1, "the clashing local task must be renamed");
}

