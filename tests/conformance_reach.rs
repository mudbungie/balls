//! SPEC-tracker-state §16 conformance — onboarding and reachability.
//!
//! Tests 4, 7, 8: a fresh `git clone` carrying only the address
//! onboards on `bl prime`; first contact with an unreachable explicit
//! tracker hard-fails while the implicit default falls back to a local
//! `git init`; a warm checkout soft-fails when the tracker goes away.

mod common;

use common::tracker::*;
use common::*;

/// Test 4 — Fresh-clone onboard. A `git clone` of a clone whose
/// committed `config.json` carries `state_url` materializes
/// `.balls/state-repo` and the symlinks on `bl prime`, and `bl ready`
/// then lists the tracker's tasks.
#[test]
#[ignore = "bl-be70 (Phase 1B-7): bl remaster XDG-aware paths — test premise relies on the legacy state_url config field"]
fn t4_fresh_clone_onboards_on_prime() {
    let code = new_bare_remote();
    let tracker = new_tracker();

    let onboard = clone_from_remote(code.path(), "onboard");
    init_in(onboard.path());
    bl(onboard.path())
        .args(["remaster", &url_of(&tracker), "--commit"])
        .assert()
        .success();
    assert_eq!(
        state_url(onboard.path()).as_deref(),
        Some(url_of(&tracker).as_str()),
        "the address must live in committed config.json, not a pointer file"
    );
    let shared = create_task(onboard.path(), "shared backlog task");
    bl(onboard.path()).arg("sync").assert().success();
    git(onboard.path(), &["push", "origin", "main"]);

    // Teammate: a plain clone with no tracker remote wired by hand.
    let teammate = clone_from_remote(code.path(), "teammate");
    assert!(!teammate.path().join(".balls/state-repo").exists());

    bl(teammate.path()).arg("prime").assert().success();
    assert!(
        teammate.path().join(".balls/state-repo/.git").exists(),
        "bl prime must materialize .balls/state-repo from committed state_url"
    );
    assert!(teammate.path().join(".balls/tasks").is_symlink());
    assert!(teammate.path().join(".balls/plugins").is_symlink());
    assert!(
        !teammate.path().join(".balls/config.json").is_symlink(),
        "config.json is a real repo file — never symlinked"
    );

    let ready = bl(teammate.path()).arg("ready").assert().success();
    let out = String::from_utf8(ready.get_output().stdout.clone()).unwrap();
    assert!(out.contains(&shared), "fresh clone must see the shared task: {out}");
}

/// Test 7a — Hard-fail explicit. First contact with an unreachable
/// explicit `state_url` aborts with a diagnostic naming the URL and
/// the three resolutions, leaving no partial checkout.
#[test]
#[ignore = "bl-be70 (Phase 1B-7): bl remaster XDG-aware paths — test premise relies on the legacy state_url config field"]
fn t7_hard_fail_unreachable_explicit() {
    let repo = new_repo();
    let url = "/no/such/explicit/tracker.git";
    seed_config(repo.path(), &[("state_url", url)]);

    let assert = bl(repo.path()).arg("init").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("could not reach"), "{stderr}");
    assert!(stderr.contains(url), "diagnostic must name the URL: {stderr}");
    assert!(stderr.contains("remaster --detach"), "diagnostic must offer detach: {stderr}");
    assert!(
        !repo.path().join(".balls/state-repo").exists(),
        "a hard-fail must leave no partial .balls/state-repo"
    );
}

/// Test 7b — Local fallback for the implicit default. First contact
/// with no `state_url` set and no reachable `origin` falls back to a
/// local `git init` of `.balls/state-repo` — a solo project is
/// offline-bootstrappable.
#[test]
#[ignore = "bl-be70 (Phase 1B-7): bl remaster XDG-aware paths — test premise relies on the legacy state_url config field"]
fn t7_local_fallback_implicit_default() {
    let repo = new_repo(); // no origin remote at all
    bl(repo.path()).arg("init").assert().success();
    assert!(
        repo.path().join(".balls/state-repo/.git").exists(),
        "implicit default with no origin must git-init .balls/state-repo locally"
    );
    create_task(repo.path(), "offline solo task");
    bl(repo.path()).arg("ready").assert().success();
}

/// Test 8 — Soft-fail warm. Once `.balls/state-repo` has materialized,
/// an unreachable tracker is a soft-fail: `bl` works from the local
/// checkout and the store stays usable.
#[test]
#[ignore = "bl-be70 (Phase 1B-7): bl remaster XDG-aware paths — test premise relies on the legacy state_url config field"]
fn t8_soft_fail_warm_checkout() {
    let tracker = new_tracker();
    let repo = new_repo();
    seed_config(repo.path(), &[("state_url", &url_of(&tracker))]);
    bl(repo.path()).arg("init").assert().success();
    assert!(repo.path().join(".balls/state-repo/.git").exists());

    // The tracker becomes unreachable after the warm checkout exists.
    git(
        &repo.path().join(".balls/state-repo"),
        &["remote", "set-url", "origin", "/no/such/tracker.git"],
    );

    // Warm + offline is a soft-fail: prime succeeds, the store works.
    bl(repo.path()).arg("prime").assert().success();
    create_task(repo.path(), "task written offline");
    bl(repo.path()).arg("list").assert().success();
}
