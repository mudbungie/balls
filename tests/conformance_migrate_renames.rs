//! Post-migration branch of SPEC §14.16 / §14.17 — the freshly-written
//! `repo.json` uses the new field names (`integrate.mode`,
//! `review.gate_command`), not the pre-XDG names. Plus secondary
//! `bl migrate` paths: target_branch drop warning (SPEC §6.7),
//! partial-migration retry (XDG already materialized), and stray
//! non-directory entries under `.balls-worktrees/`. Helpers shared
//! with the sibling migrate conformance files live in
//! `common::migrate`.

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::xdg_paths::own_tracker_checkout;
use common::migrate::{bases, bl_xdg, legacy_clone};
use common::*;
use std::fs;

#[test]
fn spec_14_16_17_post_migration_repo_json_uses_new_field_names() {
    let home = tmp();
    let (_remote, clone, url) = legacy_clone(home.path(), "dev/proj");

    // Seed the legacy config with the old field names so the migration
    // has to translate them: write delivery + review blocks into
    // `.balls/config.json` before invoking migrate.
    let cfg_path = clone.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["delivery"] = serde_json::json!({ "mode": "deferred" });
    cfg["review"] = serde_json::json!({ "pre_check": "make check" });
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
    git(&clone, &["add", ".balls/config.json"]);
    git(&clone, &["commit", "-m", "seed legacy fields", "--no-verify"]);

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();

    let enc_origin = percent_encode_component(&canonicalize_origin(&url));
    let tracker = own_tracker_checkout(&bases(home.path()), &enc_origin);
    let repo_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tracker.join(".balls/repo.json")).unwrap())
            .unwrap();

    // Old names absent.
    assert!(
        repo_json.get("delivery").is_none(),
        "delivery rename failed (still present in repo.json: {repo_json})"
    );
    let review = repo_json.get("review").expect("review block missing");
    assert!(
        review.get("pre_check").is_none(),
        "pre_check rename failed: {review}"
    );

    // New names present with the translated values.
    let integrate = repo_json.get("integrate").expect("integrate missing");
    assert_eq!(integrate.get("mode").and_then(|v| v.as_str()), Some("forge-pr"));
    assert_eq!(
        review.get("gate_command").and_then(|v| v.as_str()),
        Some("make check")
    );
}

#[test]
fn migration_warns_when_legacy_carried_repo_level_target_branch() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    let cfg_path = clone.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["target_branch"] = serde_json::json!("develop");
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
    git(&clone, &["add", ".balls/config.json"]);
    git(&clone, &["commit", "-m", "seed target_branch", "--no-verify"]);

    let out = bl_xdg(&clone, home.path()).arg("migrate").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("target_branch"),
        "expected target_branch drop warning in migrate output; got {stdout}"
    );
}

#[test]
fn migration_skips_already_materialized_xdg_tracker_checkout() {
    let home = tmp();
    let (_remote, clone, url) = legacy_clone(home.path(), "dev/proj");

    // Pre-create the XDG tracker checkout by hand — clone the legacy
    // state-repo to the nested-XDG path. Simulates a half-applied
    // migration where the state-side commit landed but the on-main
    // cleanup did not.
    let enc_origin = percent_encode_component(&canonicalize_origin(&url));
    let target = own_tracker_checkout(&bases(home.path()), &enc_origin);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let out = std::process::Command::new("git")
        .args(["clone", "-q", "--single-branch", "--branch", "balls/tasks"])
        .arg(clone.join(".balls/state-repo"))
        .arg(&target)
        .output()
        .unwrap();
    assert!(out.status.success(), "pre-seed clone failed: {}",
        String::from_utf8_lossy(&out.stderr));

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();

    // Migration must have completed despite the pre-existing checkout.
    assert!(!clone.join(".balls").exists());
}

#[test]
fn migration_ignores_non_directory_entries_under_balls_worktrees() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");

    // A stray file at `.balls-worktrees/STRAY` is not a worktree;
    // the refuse-if-dirty walk skips it and migrate proceeds.
    let wt_root = clone.join(".balls-worktrees");
    fs::create_dir_all(&wt_root).unwrap();
    fs::write(wt_root.join("STRAY"), "leftover\n").unwrap();

    bl_xdg(&clone, home.path())
        .arg("migrate")
        .assert()
        .success();
    assert!(!clone.join(".balls-worktrees").exists());
}
