//! bl-82a4 legacy compat — repos written by older `bl` (master_url in
//! the canonical config, no master.json) keep working. Their plugin
//! reads still get hub-side values (the bl-a7d9 "master wins"
//! contract preserved). An explicit `bl remaster <same-url> --commit`
//! migrates them into the new shape.

mod common;

use common::*;
use std::fs;
use std::path::Path;

fn write_legacy_config(repo: &Path, master_url: &str) {
    let cfg = serde_json::json!({
        "version": 1,
        "id_length": 4,
        "stale_threshold_seconds": 60,
        "worktree_dir": ".balls-worktrees",
        "master_url": master_url,
    });
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        serde_json::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();
}

#[test]
fn legacy_in_canonical_master_url_still_materializes_state_repo() {
    // Simulate a repo written by pre-bl-82a4 `bl`: master_url inside
    // `.balls/config.json`, no `master.json`. `bl init` followed by
    // any lifecycle command must still discover the federation via
    // the MasterPointer legacy fallback.
    let hub = new_bare_remote();
    let alice = new_repo();
    write_legacy_config(alice.path(), hub.path().to_string_lossy().as_ref());

    bl(alice.path()).arg("init").assert().success();
    assert!(
        alice.path().join(".balls/state-repo/.git").exists(),
        "legacy in-canonical master_url must still trigger state-repo materialization"
    );
}

#[test]
fn remaster_commit_migrates_legacy_shape_to_pointer_and_symlinks() {
    let hub = new_bare_remote();
    let alice = new_repo();
    write_legacy_config(alice.path(), hub.path().to_string_lossy().as_ref());
    bl(alice.path()).arg("init").assert().success();

    // Re-run remaster --commit with the same URL: idempotent migration
    // into the new shape.
    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    assert!(
        alice.path().join(".balls/master.json").exists(),
        "migration must materialize the .balls/master.json pointer"
    );
    let pointer: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(alice.path().join(".balls/master.json")).unwrap(),
    )
    .unwrap();
    assert!(
        pointer.get("master_url").is_some(),
        "pointer must carry master_url"
    );
    assert!(
        alice.path().join(".balls/config.json").is_symlink(),
        "migration must swap the canonical for a symlink"
    );
}
