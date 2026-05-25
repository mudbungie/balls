//! `bl migrate` refuses-if-dirty (SPEC §11.1 / §11.3) and the
//! federated tracker.json write (SPEC §11.1 + §6.1). Sibling to
//! `conformance_migrate.rs`; helpers live in `common::migrate` so both
//! files share one source of legacy-clone setup.

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::xdg_paths::own_tracker_checkout;
use common::migrate::{bases, bl_xdg, legacy_clone, origin_url_of};
use common::*;
use std::fs;

#[test]
fn migration_refuses_uncommitted_changes_on_main() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");

    // Dirty the working tree on main.
    fs::write(clone.join("README"), "edited but not committed\n").unwrap();

    let out = bl_xdg(&clone, home.path()).arg("migrate").output().unwrap();
    assert!(!out.status.success(), "dirty main must abort migrate");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("uncommitted changes"),
        "expected uncommitted-changes diagnostic; got: {stderr}"
    );
    // Legacy layout still in place after the refused migration.
    assert!(clone.join(".balls/config.json").exists());
    assert!(clone.join(".balls/state-repo").exists());
}

#[test]
fn migration_refuses_when_a_task_worktree_is_dirty() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");

    // Create a fake "worktree" colocation — a git worktree under
    // `.balls-worktrees/bl-abcd/` with an uncommitted edit. The
    // migration's refuse-if-dirty path walks each entry; this
    // exercises the inner branch.
    git(&clone, &["branch", "work/bl-abcd"]);
    let wt = clone.join(".balls-worktrees/bl-abcd");
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(&clone, &["worktree", "add", "-q", wt.to_str().unwrap(), "work/bl-abcd"]);
    std::fs::write(wt.join("dirty.txt"), "uncommitted\n").unwrap();

    let out = bl_xdg(&clone, home.path()).arg("migrate").output().unwrap();
    assert!(!out.status.success(), "dirty worktree must abort migrate");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("worktree") && stderr.contains("uncommitted"),
        "expected worktree-dirty diagnostic; got: {stderr}"
    );
}

#[test]
fn migration_succeeds_when_a_task_worktree_is_clean() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");

    // Clean worktree exists at `.balls-worktrees/bl-clean/` — migrate
    // moves it into the XDG path via `git worktree move`.
    git(&clone, &["branch", "work/bl-clean"]);
    let wt = clone.join(".balls-worktrees/bl-clean");
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(&clone, &["worktree", "add", "-q", wt.to_str().unwrap(), "work/bl-clean"]);

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();

    let bases = bases(home.path());
    let nested = common::migrate::nested(&clone);
    let target = bases
        .state_root()
        .join("worktrees")
        .join(nested)
        .join("bl-clean");
    assert!(target.exists(), "worktree was not moved to XDG: {}", target.display());
    assert!(
        !clone.join(".balls-worktrees").exists(),
        "in-repo .balls-worktrees should be removed after migration"
    );
}

#[test]
fn migration_writes_tracker_json_when_legacy_carried_state_url() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");

    // Plant a `state_url` in legacy config so the migration writes
    // tracker.json on the new side.
    let cfg_path = clone.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["state_url"] = serde_json::json!("git@host:org/tracker.git");
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
    git(&clone, &["add", ".balls/config.json"]);
    git(&clone, &["commit", "-m", "seed federated config", "--no-verify"]);

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();

    let enc_origin =
        percent_encode_component(&canonicalize_origin(&origin_url_of(&clone)));
    let tracker = own_tracker_checkout(&bases(home.path()), &enc_origin);
    let tj_path = tracker.join(".balls/tracker.json");
    assert!(
        tj_path.exists(),
        "federated migration must write tracker.json"
    );
    let tj: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&tj_path).unwrap()).unwrap();
    assert_eq!(
        tj.get("state_url").and_then(|v| v.as_str()),
        Some("git@host:org/tracker.git")
    );
}
