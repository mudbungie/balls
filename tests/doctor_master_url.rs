//! `bl doctor` — master_url-mode drift checks. Split out of
//! `tests/doctor.rs` for line-budget; the legacy-mode and structural
//! probes stay there. Every check is read-only: the test fixture
//! simulates drift, doctor names it, the disk is otherwise untouched.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// Run `bl doctor` and return stdout. Asserts exit 0 — doctor is
/// read-only and never fails the process, the verdict is in the text.
fn doctor(cwd: &Path) -> String {
    let out = bl(cwd).arg("doctor").output().expect("bl doctor");
    assert!(out.status.success(), "doctor must exit 0 (read-only)");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Set `master_url` to a bare hub and materialize `.balls/state-repo/`.
/// Returns `(repo, hub, hub_url)`. Subsequent edits to `state-repo`
/// simulate drift without re-triggering auto-provisioning, because
/// `auto_provision_master` only re-runs `ensure` when the `.git`
/// directory is fully absent.
fn master_url_repo() -> (common::Repo, common::Repo, String) {
    let hub = new_bare_remote();
    let repo = new_repo();
    init_in(repo.path());
    let hub_url = hub.path().to_string_lossy().to_string();
    bl(repo.path())
        .arg("remaster")
        .arg(&hub_url)
        .arg("--commit")
        .assert()
        .success();
    (repo, hub, hub_url)
}

#[test]
fn master_url_healthy_state_repo_is_silent() {
    let (repo, _hub, _url) = master_url_repo();
    // In master_url mode the state-repo is a full clone, not a linked
    // worktree. The legacy `linked_worktree_ok` check would always fail
    // against a `.git` directory — the regression bl-c61b fixes.
    assert!(repo.path().join(".balls/state-repo/.git").is_dir());
    let out = doctor(repo.path());
    assert!(out.contains("no problems detected"), "got: {out}");
}

#[test]
fn master_url_broken_state_repo_names_correct_path_and_fix() {
    let (repo, _hub, _url) = master_url_repo();
    // HEAD removed but `.git/` directory survives, so
    // `auto_provision_master` skips re-materialization and doctor
    // gets to surface the drift.
    fs::remove_file(repo.path().join(".balls/state-repo/.git/HEAD")).unwrap();
    let out = doctor(repo.path());
    assert!(
        out.contains("state-repo") && out.contains("not a valid git clone"),
        "expected master_url-specific wording, got: {out}"
    );
    assert!(
        out.contains(".balls/state-repo"),
        "diagnostic must name the state-repo path, got: {out}"
    );
    assert!(
        out.contains("bl prime"),
        "fix must reference `bl prime` (master_url re-materialization), got: {out}"
    );
    assert!(
        !out.contains("not a valid linked git worktree"),
        "must NOT use legacy linked-worktree wording, got: {out}"
    );
}

#[test]
fn master_url_origin_drift_is_flagged() {
    let (repo, _hub, hub_url) = master_url_repo();
    // Simulate the "user edited master_url in committed config" case:
    // point state-repo's origin at a different URL, leaving committed
    // master_url alone — drift in the direction users actually create.
    let drifted = "git@example.invalid:other/hub.git";
    git(
        &repo.path().join(".balls/state-repo"),
        &["remote", "set-url", "origin", drifted],
    );
    let out = doctor(repo.path());
    assert!(
        out.contains("does not match") && out.contains(&hub_url) && out.contains(drifted),
        "expected drift diagnostic naming both URLs, got: {out}"
    );
    assert!(
        out.contains("bl remaster") || out.contains("master_url"),
        "fix must name the remediation, got: {out}"
    );
}

#[test]
fn master_url_state_repo_with_no_origin_is_flagged() {
    let (repo, _hub, _url) = master_url_repo();
    git(
        &repo.path().join(".balls/state-repo"),
        &["remote", "remove", "origin"],
    );
    let out = doctor(repo.path());
    assert!(
        out.contains("no `origin` remote"),
        "expected no-origin diagnostic, got: {out}"
    );
    assert!(out.contains("bl prime"), "expected bl prime hint, got: {out}");
}

#[test]
fn master_url_missing_tasks_symlink_is_flagged() {
    let (repo, _hub, _url) = master_url_repo();
    // The state-repo is healthy; only the convenience symlink is gone.
    // Removing the symlink itself doesn't disturb auto_provision_master
    // (which gates on `.balls/state-repo/.git`).
    fs::remove_file(repo.path().join(".balls/tasks")).unwrap();
    let out = doctor(repo.path());
    assert!(
        out.contains(".balls/tasks") && out.contains("convenience symlink is missing"),
        "expected missing-symlink diagnostic, got: {out}"
    );
    assert!(out.contains("bl remaster"), "expected remaster hint, got: {out}");
}

#[test]
fn master_url_stale_tasks_symlink_target_is_flagged() {
    let (repo, _hub, _url) = master_url_repo();
    // Simulate a legacy→master_url remaster where the old symlink
    // pointing at `worktree/.balls/tasks` was never repointed — the
    // exact stale-target case bl-773e covers on the repair side.
    let link = repo.path().join(".balls/tasks");
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("worktree/.balls/tasks", &link).unwrap();
    let out = doctor(repo.path());
    assert!(
        out.contains("worktree/.balls/tasks") && out.contains("state-repo/.balls/tasks"),
        "expected stale-target diagnostic naming both targets, got: {out}"
    );
    assert!(out.contains("repoint"), "expected repoint hint, got: {out}");
}

#[test]
fn master_url_stray_non_symlink_at_tasks_path_is_flagged() {
    let (repo, _hub, _url) = master_url_repo();
    // Replace the symlink with a regular file: ensure_tasks_symlink
    // refuses to overwrite this — doctor must surface it so the user
    // knows to remove the stray entry by hand.
    let link = repo.path().join(".balls/tasks");
    fs::remove_file(&link).unwrap();
    fs::write(&link, "stray\n").unwrap();
    let out = doctor(repo.path());
    assert!(
        out.contains("stray file or directory"),
        "expected stray-non-symlink diagnostic, got: {out}"
    );
    assert!(out.contains("remove"), "expected removal hint, got: {out}");
}
