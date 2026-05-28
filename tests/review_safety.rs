//! Regression tests for bl-0dc3: `bl review` must never deliver
//! `.balls/{local,tasks,worktree}` runtime symlinks to the
//! integration branch, and any failure mid-review must rewind main
//! to its pre-review tip so a returned error reflects an unmutated
//! integration branch.

mod common;

use common::*;
use std::os::unix::fs::PermissionsExt;

fn assert_no_runtime_paths(repo_root: &std::path::Path, ref_: &str) {
    let changed = git(repo_root, &["show", "--name-only", "--format=", ref_]);
    for p in [".balls/local", ".balls/tasks", ".balls/state-repo"] {
        assert!(
            !changed.lines().any(|l| l.starts_with(p)),
            "runtime path {p} leaked into commit {ref_}:\n{changed}"
        );
    }
}

#[test]
fn review_never_stages_balls_runtime_symlinks() {
    // The runtime symlinks at `.balls/{local,tasks,worktree}` are
    // created by `bl claim`. A normal `bl review` over a user file
    // must produce a squash commit that contains the user file
    // only — never the symlinks, regardless of how `git add -A`
    // behaved in the worktree.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    // Under XDG (Phase 1B) claim no longer plants `.balls/` symlinks
    // in the worktree — there is nothing for `git add -A` to stage.
    // The legacy regression this test guards against (symlinks
    // sneaking into the squash) is moot under XDG; the assertion
    // is unconditionally true here.
    std::fs::write(wt.join("user.txt"), "real change").unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "ship"])
        .assert()
        .success();

    assert_no_runtime_paths(repo.path(), "HEAD");
    let changed = git(repo.path(), &["show", "--name-only", "--format=", "HEAD"]);
    assert!(
        changed.contains("user.txt"),
        "user change missing: {changed}"
    );
}

#[test]
fn review_scrubs_runtime_paths_force_tracked_on_work_branch() {
    // Backstop for repos whose `.gitignore` predates `bl init` —
    // even when a prior commit on `work/<id>` force-tracks a
    // runtime path, the delivery commit must not contain it.
    // `add_user_changes` unstages the runtime paths before the wip
    // commit, so the squash carries a clean delta.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);

    std::fs::write(wt.join("feat.txt"), "real").unwrap();
    // `.balls/local` is no longer auto-symlinked (bl-51a5); plant a
    // local dir manually so the force-add exercises the same
    // runtime-path scrubbing backstop.
    std::fs::create_dir_all(wt.join(".balls/local")).unwrap();
    std::fs::write(wt.join(".balls/local/marker"), "x").unwrap();
    git(&wt, &["add", "-f", "feat.txt", ".balls/local"]);
    git(&wt, &["commit", "-m", "wip with runtime tracked"]);

    bl(repo.path())
        .args(["review", &id, "-m", "ship cleanly"])
        .assert()
        .success();

    assert_no_runtime_paths(repo.path(), "HEAD");
    let changed = git(repo.path(), &["show", "--name-only", "--format=", "HEAD"]);
    assert!(changed.contains("feat.txt"), "user file missing: {changed}");
    bl(repo.path()).args(["list"]).assert().success();
}

#[test]
fn review_rewinds_main_on_post_squash_failure() {
    // Atomicity: when `bl review` fails after the squash has landed
    // on main, the integration branch must be reset to its
    // pre-review tip. Driving `--sync` against an unreachable origin
    // forces the required push to error after the squash and
    // state-branch flip both succeed.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("feat.txt"), "real").unwrap();

    let pre_main = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();

    // Install a pre-receive hook on the bare remote that rejects
    // every push, forcing the post-squash main push to error. The
    // clone's origin URL is unchanged so XDG discovery still resolves
    // the tracker checkout, and `bl list` after the rewind keeps
    // working. (Under legacy `bl init` this test exercised the
    // no-origin path; under XDG `bl init` requires origin, so a
    // remote that loudly rejects pushes is the equivalent.)
    let origin = repo
        .origin_remote
        .as_ref()
        .expect("new_repo provides an origin remote")
        .path();
    let hook = origin.join("hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\necho 'remote: rejected'\nexit 1\n").unwrap();
    let mut perms = std::fs::metadata(&hook).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&hook, perms).unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "should rewind", "--sync"])
        .assert()
        .failure();

    let post_main = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        pre_main, post_main,
        "main was not rewound after a failed review:\n  pre={pre_main}\n  post={post_main}"
    );
    bl(repo.path()).args(["list"]).assert().success();
}
