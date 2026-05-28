//! SPEC-tracker-state §16 conformance — non-default `state_branch`.
//!
//! §5 says a tracker address is the pair `(state_url, state_branch)`,
//! and §8 specifies `bl remaster <url> [--branch B]` as the CLI that
//! writes it. bl-3f59 wired the branch end to end (push/fetch/merge
//! refspecs, plus the `--branch` flag); this test gates that wiring,
//! exercising create → claim → review → close + sync round-trip on a
//! custom branch and asserting the lifecycle traffic targets `B` —
//! not the default `balls/tasks`.

mod common;

use common::tracker::*;
use common::*;
use std::path::Path;

const PROJECT_BRANCH: &str = "project-x";

/// `bl init` against a tracker URL with `state_branch=project-x`:
/// `.balls/state-repo` materializes on `project-x`, not `balls/tasks`.
fn clone_on_custom_branch(tracker_url: &str, name: &str) -> Repo {
    let ws = new_repo();
    seed_config(
        ws.path(),
        &[("state_url", tracker_url), ("state_branch", PROJECT_BRANCH)],
    );
    bl(ws.path())
        .arg("init")
        .env("BALLS_IDENTITY", name)
        .assert()
        .success();
    ws
}

/// Test §16.* — Non-default `state_branch` round-trip. Lifecycle traffic
/// targets the configured branch end to end; the default `balls/tasks`
/// is never created on the tracker.
#[test]
#[ignore = "bl-be70 (Phase 1B-7): bl remaster XDG-aware paths — test premise relies on the legacy state_url config field"]
fn state_branch_round_trip_on_custom_branch() {
    let tracker = new_bare_remote();
    let code = new_bare_remote();

    let ws = clone_from_remote(code.path(), "ws");
    // The clone has `origin = code`. Layer `state_url = tracker` and
    // `state_branch = project-x` on top via committed config, then init.
    seed_config(
        ws.path(),
        &[
            ("state_url", &url_of(&tracker)),
            ("state_branch", PROJECT_BRANCH),
        ],
    );
    init_in(ws.path());
    git(ws.path(), &["push", "origin", "main"]);

    // Materialization: state-repo HEAD is the configured branch — not
    // the SPEC default — confirming `state_repo::ensure` checked out
    // what the config asked for, not what `git_state::STATE_BRANCH`
    // used to hardcode.
    let head = git_state(ws.path(), &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(
        head.trim(),
        PROJECT_BRANCH,
        "state-repo HEAD must be the configured state_branch",
    );

    // Lifecycle: create → claim → review → close, then sync.
    let id = create_task(ws.path(), "task on the custom branch");
    let claim = bl(ws.path()).args(["claim", &id]).output().unwrap();
    assert!(claim.status.success(), "{}", String::from_utf8_lossy(&claim.stderr));
    let wt = String::from_utf8(claim.stdout).unwrap().trim().to_string();
    std::fs::write(Path::new(&wt).join("feature.rs"), "code\n").unwrap();
    bl(Path::new(&wt))
        .args(["review", &id, "-m", "deliver custom-branch feature"])
        .assert()
        .success();
    bl(ws.path())
        .args(["close", &id, "-m", "shipped on custom branch"])
        .assert()
        .success();
    bl(ws.path()).arg("sync").assert().success();

    // The tracker carries `project-x` and never gained a `balls/tasks`
    // ref. If `claim_push`/`claim_sync`/`commands::sync` had still
    // hardcoded the default — the bl-022c gate's exact failure mode —
    // the clone's push would have created `balls/tasks` here.
    assert!(
        git_ok(
            tracker.path(),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{PROJECT_BRANCH}")],
        ),
        "the tracker must carry refs/heads/{PROJECT_BRANCH} after sync",
    );
    assert!(
        !git_ok(
            tracker.path(),
            &["rev-parse", "--verify", "--quiet", "refs/heads/balls/tasks"],
        ),
        "the tracker must not have a stray balls/tasks branch",
    );

    // The state lifecycle commits land on `project-x`, including the
    // task-archival close — proof that close_and_archive committed to
    // the configured branch (HEAD in the checkout) rather than a
    // hardcoded default.
    let tracker_log = git(
        tracker.path(),
        &["log", "--format=%s", PROJECT_BRANCH],
    );
    assert!(
        tracker_log.contains(&id),
        "the task-state lifecycle must reach the tracker's {PROJECT_BRANCH}: {tracker_log}",
    );

    // bl-3f59 closure: a second clone cloned from the same tracker
    // sees the closed task in its archive view, proving the
    // archive-recovery path resolves through `HEAD` rather than a
    // hardcoded `balls/tasks`.
    let peer = clone_on_custom_branch(&url_of(&tracker), "peer");
    let closed = bl(peer.path())
        .args(["list", "--closed", "--json"])
        .output()
        .unwrap();
    assert!(closed.status.success());
    let json: serde_json::Value = serde_json::from_slice(&closed.stdout).unwrap();
    let ids: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t.get("id").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        ids.contains(&id.as_str()),
        "peer clone must see {id} in --closed view: {ids:?}",
    );
}

/// Test §16.* — `bl remaster <url> --branch B` writes the address.
/// SPEC §6.1 names the field; the redirect file is `tracker.json` on
/// the code repo's own `balls/tasks` checkout (bl-be70).
#[test]
fn remaster_branch_flag_writes_state_branch() {
    use balls::encoding::{canonicalize_origin, percent_encode_component};
    use balls::tracker_json::TrackerJson;
    use balls::xdg_paths::own_tracker_checkout;

    let xdg = new_xdg_repo();
    let tracker = new_bare_remote();
    let tracker_url = tracker.path().to_string_lossy().into_owned();

    bl(xdg.clone.path())
        .args(["remaster", &tracker_url, "--branch", PROJECT_BRANCH, "--commit"])
        .assert()
        .success();

    let enc = percent_encode_component(&canonicalize_origin(
        &xdg.remote.path().to_string_lossy(),
    ));
    let own = own_tracker_checkout(&test_xdg_bases(), &enc);
    let tj_path = own.join(".balls/tracker.json");
    let parsed = TrackerJson::from_json(&std::fs::read_to_string(&tj_path).unwrap()).unwrap();
    assert_eq!(parsed.state_url, tracker_url);
    assert_eq!(
        parsed.state_branch.as_deref(),
        Some(PROJECT_BRANCH),
        "remaster --branch B must persist state_branch in tracker.json",
    );

    // --detach removes the file, returning the clone to solo mode.
    bl(xdg.clone.path())
        .args(["remaster", "--detach", "--commit"])
        .assert()
        .success();
    assert!(
        !tj_path.exists(),
        "remaster --detach must remove tracker.json (solo again)",
    );
}
