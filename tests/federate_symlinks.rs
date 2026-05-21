//! bl-82a4 — `.balls/config.json` and `.balls/plugins/` materialize as
//! symlinks into the hub state-repo on `bl remaster <url> --commit`,
//! and `.balls/master.json` carries the bootstrap pointer. End-to-end
//! coverage at the CLI surface.

mod common;

use common::*;
use std::fs;

fn read_pointer_master_url(repo: &std::path::Path) -> Option<String> {
    let s = fs::read_to_string(repo.join(".balls/master.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get("master_url")?.as_str().map(String::from)
}

#[test]
fn remaster_commit_writes_master_json_pointer() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    let url = hub.path().to_string_lossy().to_string();

    bl(alice.path())
        .arg("remaster")
        .arg(&url)
        .arg("--commit")
        .assert()
        .success();

    assert_eq!(
        read_pointer_master_url(alice.path()).as_deref(),
        Some(url.as_str()),
        ".balls/master.json must carry the master_url"
    );
}

#[test]
fn remaster_commit_swaps_canonical_for_symlink() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    let canonical = alice.path().join(".balls/config.json");
    assert!(
        canonical.is_symlink(),
        ".balls/config.json must become a symlink under federation"
    );
    let target = fs::read_link(&canonical).unwrap();
    assert!(
        target.to_string_lossy().contains("state-repo"),
        "symlink target must point into state-repo, got {}",
        target.display()
    );
}

#[test]
fn remaster_commit_swaps_plugins_for_symlink() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    let plugins = alice.path().join(".balls/plugins");
    assert!(plugins.is_symlink(), ".balls/plugins must become a symlink");
}

#[test]
fn remaster_commit_is_idempotent() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    let url = hub.path().to_string_lossy().to_string();

    bl(alice.path())
        .arg("remaster")
        .arg(&url)
        .arg("--commit")
        .assert()
        .success();
    // Second invocation must short-circuit cleanly (no error).
    bl(alice.path())
        .arg("remaster")
        .arg(&url)
        .arg("--commit")
        .assert()
        .success()
        .stdout(predicates::str::contains("already federated"));
}

#[test]
fn remaster_url_bootstraps_a_non_initted_repo() {
    // A fresh `git init`'d repo with no `.balls/` — `bl remaster <url>
    // --commit` is the one-shot federation entry point (bl-82a4).
    let hub = new_bare_remote();
    let fresh = new_repo();
    assert!(!fresh.path().join(".balls").exists(), "precondition: no .balls/");

    bl(fresh.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    assert!(fresh.path().join(".balls/master.json").exists());
    assert!(fresh.path().join(".balls/config.json").is_symlink());
    assert!(fresh.path().join(".balls/state-repo/.git").exists());
}

#[test]
fn remaster_url_on_non_initted_repo_requires_commit() {
    let hub = new_bare_remote();
    let fresh = new_repo();
    bl(fresh.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .assert()
        .failure()
        .stderr(predicates::str::contains("needs --commit"));
}

#[test]
fn second_repo_federating_to_same_hub_discards_duplicate_plugin() {
    use common::plugin::configure_plugin;
    let hub = new_bare_remote();
    let url = hub.path().to_string_lossy().to_string();

    // Repo A federates first; its `mock` plugin is promoted to the hub.
    let alice = new_repo();
    init_in(alice.path());
    configure_plugin(alice.path());
    bl(alice.path())
        .arg("remaster")
        .arg(&url)
        .arg("--commit")
        .assert()
        .success();

    // Repo B also configures `mock`, then federates to the same hub —
    // the hub already owns `mock`, so B's entry is discarded.
    let bob = new_repo();
    init_in(bob.path());
    configure_plugin(bob.path());
    let out = bl(bob.path())
        .arg("remaster")
        .arg(&url)
        .arg("--commit")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(
        stdout.contains("discarded project-side plugin entries (hub wins): mock"),
        "expected discard summary, got: {stdout}"
    );
}

#[test]
fn detach_reverses_the_symlink_dance() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    bl(alice.path())
        .arg("remaster")
        .arg("--detach")
        .assert()
        .success();

    let canonical = alice.path().join(".balls/config.json");
    assert!(
        !canonical.is_symlink(),
        "detach must restore .balls/config.json as a real file"
    );
    assert!(
        canonical.exists(),
        "detach must leave a real config.json behind"
    );
    assert!(
        !alice.path().join(".balls/master.json").exists()
            || fs::read_to_string(alice.path().join(".balls/master.json"))
                .unwrap()
                .trim()
                == "{}",
        "detach must clear the master.json pointer"
    );

    // The detached repo must remain usable as a standalone store.
    bl(alice.path()).arg("prime").arg("--as").arg("t").assert().success();
    bl(alice.path()).arg("list").assert().success();
}
