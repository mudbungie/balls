//! bl-7582: the close path's THREE delivery gates, driven end-to-end through the
//! real `bl-delivery` binary (the `close.pre` op), not the unit seam. Each is a
//! reason the squash must NOT land, and each must abort BEFORE core seals the
//! task — so the observable outcome is: `close.pre` exits non-zero, integration
//! (`main`) never moves, and the work worktree is left in a state the agent can
//! fix from.
//!
//! - The project's own `pre-commit` hook (bl-ee85): a failing hook aborts before
//!   the seal with the worktree still up; a passing hook delivers.
//! - A `MERGE_HEAD` half-merge in the work worktree (bl-33db): delivery refuses
//!   to conclude it (capture would silently resolve every conflict work-side —
//!   the resurrection door) and names the risk.
//! - A target that advanced after claim, clean or conflicting (bl-a1a4): the
//!   source is STALE, and delivery refuses before it merges, gates, squashes or
//!   moves a ref. Reconciliation is the source owner's; delivery only validates
//!   the tree they tested and advances the target to it.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use balls::delivery_path::worktree_path;
use balls::layout::Xdg;
use predicates::str::contains;
use tempfile::TempDir;

use crate::{change_dir, delivery, git, post, pre, project};

/// Install `script` as the project repo's shared `pre-commit` hook (every linked
/// worktree resolves `.git/hooks` via the common dir), `mode`-permissioned.
fn install_hook(root: &Path, script: &str, mode: u32) {
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, script).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(mode)).unwrap();
}

/// `git -C cwd <args>` stdout, trimmed — a raw read that never asserts (used for
/// merges that intentionally exit non-zero and for state probes).
fn out(cwd: &Path, args: &[&str]) -> String {
    let o = Command::new("git").current_dir(cwd).args(args).output().unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// The subject of `main`'s tip in the project repo.
fn main_subject(root: &Path) -> String {
    out(root, &["log", "-1", "--format=%s", "main"])
}

/// Does the worktree at `cwd` have a merge in progress?
fn has_merge_head(cwd: &Path) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--verify", "--quiet", "MERGE_HEAD"])
        .output()
        .unwrap()
        .status
        .success()
}

/// Claim `bl-x` and return `(TempDir, home, root, invocation, worktree)`, with
/// the code worktree materialized on `main`'s seed.
fn claimed() -> (TempDir, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap().to_string();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", &inv, "bl-x");
    delivery(&root, &home, "claim", "post", &post(&inv, "bl-x", "Add feature")).assert().success();
    assert!(wt.join("seed.txt").exists());
    // leak the TempDir owner back to the caller so paths stay live
    let owner = tmp;
    (owner, home, root, wt)
}

#[test]
fn a_failing_pre_commit_hook_aborts_before_the_seal_with_the_worktree_up() {
    // bl-ee85: the squash is plumbing and would bypass the porcelain pre-commit
    // gate; delivery restores it on the tree it is about to land. A failing hook aborts
    // the close BEFORE the seal — main never moves and the worktree stays up for
    // the fix, so core (which seals only on a clean close.pre) leaves the task
    // claimed.
    let (tmp, home, root, wt) = claimed();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "broken\n").unwrap();
    install_hook(&root, "#!/bin/sh\nexit 1\n", 0o755);

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("delivery gate"));

    assert_eq!(main_subject(&root), "seed", "integration must not move on a failed gate");
    assert!(wt.join("feature.txt").exists(), "the worktree stays up for the fix");
}

#[test]
fn a_passing_pre_commit_hook_delivers() {
    // The mirror: a hook that succeeds (and proves it ran in the WORKTREE by
    // requiring the work's own file in $PWD) lets the squash land.
    let (tmp, home, root, wt) = claimed();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    install_hook(&root, "#!/bin/sh\ntest -f feature.txt\n", 0o755);

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature")).assert().success();

    assert_eq!(main_subject(&root), "Add feature [bl-x]");
}

#[test]
fn a_merge_head_in_the_work_worktree_refuses_naming_the_bl_33db_resurrection() {
    // The agent left a half-merge (started reconciling the target by hand,
    // never finished). capture's `add -A` + commit over a MERGE_HEAD would CONCLUDE it,
    // silently resolving every modify/delete work-side — the bl-33db
    // resurrection. Delivery refuses and names the risk; main never moves.
    let (tmp, home, root, wt) = claimed();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "work side\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "work commit"]);

    // Integration advances on a DIFFERENT file, then the agent starts merging it
    // in and stops — `--no-commit` holds the merge open, leaving MERGE_HEAD.
    fs::write(root.join("mainfile.txt"), "landed meanwhile\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "main edit"]);
    Command::new("git")
        .current_dir(&wt)
        .args(["merge", "--no-commit", "--no-ff", "main"])
        .output()
        .unwrap();
    assert!(has_merge_head(&wt), "test setup: the half-merge must leave MERGE_HEAD");

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("merge is in progress"))
        .stderr(contains("bl-33db"));

    assert_eq!(main_subject(&root), "main edit", "no squash lands over a refused half-merge");
    assert!(has_merge_head(&wt), "delivery leaves the half-merge for the agent to resolve");
}

#[test]
fn a_clean_target_advance_refuses_then_delivers_once_the_closer_incorporates_it() {
    // Acceptance 1 (bl-a1a4). `main` advanced on a file the work branch never
    // touched — git would have folded it in silently, and the gate would then
    // have run on a tree nobody had ever built. Delivery refuses instead, and
    // the retry lands only after the closer merged and tested it themselves.
    let (tmp, home, root, wt) = claimed();
    let inv = root.to_str().unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "work adds feature"]);

    fs::write(root.join("disjoint.txt"), "landed meanwhile\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "main gains a disjoint file"]);
    let before = out(&root, &["rev-list", "--count", "main"]);
    let branch_before = out(&root, &["rev-parse", "work/bl-x"]);

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("stale source"))
        .stderr(contains("is not yet in work/bl-x"))
        .stderr(contains("Merge or rebase main into the work/bl-x worktree"));

    assert_eq!(main_subject(&root), "main gains a disjoint file", "the target ref never moved");
    assert_eq!(out(&root, &["rev-list", "--count", "main"]), before, "main gained no commit");
    assert_eq!(out(&root, &["rev-parse", "work/bl-x"]), branch_before, "no merge commit was created");
    assert!(!has_merge_head(&wt), "delivery started no merge");

    // The closer reconciles in their own worktree, then closes.
    git(&wt, &["merge", "-q", "--no-edit", "main"]);
    let change = change_dir(tmp.path(), "change-retry");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature")).assert().success();
    assert_eq!(main_subject(&root), "Add feature [bl-x]");
    assert_eq!(out(&root, &["show", "main:disjoint.txt"]), "landed meanwhile");
    assert_eq!(out(&root, &["show", "main:feature.txt"]), "shipped");
}

#[test]
fn a_conflicting_target_advance_refuses_identically_and_leaves_no_half_merge() {
    // Acceptance 2, in the bl-33db shape: work DELETES a file the target then
    // MODIFIES. That used to reach delivery's own fold and abort on the
    // modify/delete conflict. Now the advance alone refuses it — same voice as
    // the clean advance, because delivery never attempted reconciliation in
    // either case and so has no conflict of its own to report.
    let (tmp, home, root, wt) = claimed();
    let inv = root.to_str().unwrap();
    git(&wt, &["rm", "-q", "seed.txt"]);
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "delete seed, add feature"]);

    fs::write(root.join("seed.txt"), "changed on main\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "main modifies seed"]);
    let before = out(&root, &["rev-list", "--count", "main"]);

    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("stale source"))
        .stderr(contains("is not yet in work/bl-x"));

    assert_eq!(main_subject(&root), "main modifies seed", "no squash on a stale source");
    assert_eq!(out(&root, &["rev-list", "--count", "main"]), before, "main gained no commit");
    assert!(!has_merge_head(&wt), "delivery started no merge, so none is half-done");
    assert_eq!(out(&wt, &["status", "--porcelain"]), "", "the worktree is untouched");
    assert!(!wt.join("seed.txt").exists(), "the work-side deletion stands");
}
