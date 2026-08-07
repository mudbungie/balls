//! §11.1 attempt tests (bl-4eac) on throwaway project repos — the nine shapes
//! the ruling names: target deletion, bare repositories, concurrent attempts,
//! retry after crash, a stale target, gate failure, rejected retention, explicit
//! discard, and ordinary claim/close parity. Shares the `project`/`tip` fixtures
//! with the delivery act tests.

#![cfg(unix)]

use super::*;
use crate::delivery_repo::tests::{project, tip};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// An [`Xdg`] rooted in `tmp` — the attempt worktrees land under
/// `<tmp>/state/balls/attempts/<mirrored project path>/<handle>/`.
fn xdg(tmp: &TempDir) -> Xdg {
    let state = tmp.path().join("state");
    Xdg::with(tmp.path(), None, Some(state.to_str().unwrap()))
}

/// Commit `content` as `name` inside `wt`.
fn work(wt: &Path, name: &str, content: &str) {
    fs::write(wt.join(name), content).unwrap();
    Project::run(wt, &["add", "-A"]).unwrap();
    Project::run(wt, &["commit", "-q", "-m", &format!("add {name}")]).unwrap();
}

#[test]
fn an_attempt_forks_a_private_worktree_at_the_target_tip_and_delivers_by_the_one_law() {
    let (tmp, root, p) = project();
    let before = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();

    let target = p.target(None).unwrap(); // the integration branch, not hardcoded `main`
    let a = Attempt::open(&root, &xdg(&tmp), &target).unwrap();

    // A private, write-capable checkout of its own — outside the project tree.
    assert!(a.worktree().join("seed.txt").exists());
    assert!(a.worktree().starts_with(tmp.path().join("state/balls/attempts")));
    assert!(a.handle().starts_with("at-"), "opaque handle, unmistakable for a ball id: {}", a.handle());
    assert_eq!(a.base(), before, "the attempt starts from the exact target commit");

    work(a.worktree(), "candidate.txt", "one\n");
    assert_ne!(a.tip().unwrap(), before, "base..tip IS the project diff, no ref name spelled");

    let d = a.deliver("Try the thing", Some("why this way")).unwrap();

    // The identities, and nothing stored to produce them.
    let landed = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();
    assert_eq!(d.target, "main");
    assert_eq!(d.base, before);
    assert_eq!(d.source, Some(a.tip().unwrap()));
    assert_eq!(d.commit, Some(landed));
    // The squash is tagged with the HANDLE — acceptance is the target's own
    // history, never a stored winner field.
    assert_eq!(tip(&root), format!("Try the thing [{}]", a.handle()));
    let body = Project::run(&root, &["log", "-1", "--format=%b", "main"]).unwrap();
    assert!(body.contains("why this way") && body.contains("add candidate.txt"), "{body}");
    assert_eq!(Project::run(&root, &["show", "main:candidate.txt"]).unwrap(), "one\n");
}

#[test]
fn a_landing_sibling_makes_every_other_attempt_stale_until_its_owner_incorporates() {
    // Concurrent attempts + the stale-target refusal: one cohort, `(target,
    // base)`, two members. After one lands, every sibling is stale BY
    // CONSTRUCTION and bl-a1a4 refuses it — sequential synthesis falls out of
    // the delivery law instead of needing a merge queue.
    let (tmp, root, p) = project();
    let dirs = xdg(&tmp);
    let target = p.target(None).unwrap();
    let first = Attempt::open(&root, &dirs, &target).unwrap();
    let second = Attempt::open(&root, &dirs, &target).unwrap();
    assert_ne!(first.handle(), second.handle(), "two attempts never name one ref");
    assert_ne!(first.worktree(), second.worktree(), "and never share a worktree or index");
    assert_eq!(first.base(), second.base(), "same cohort: same (target, base)");

    work(first.worktree(), "a.txt", "a\n");
    work(second.worktree(), "b.txt", "b\n");
    first.deliver("A", None).unwrap();

    let err = second.deliver("B", None).unwrap_err().to_string();
    assert!(err.contains("stale source"), "{err}");
    assert_eq!(tip(&root), format!("A [{}]", first.handle()), "B moved nothing");

    // The source owner reconciles in its OWN worktree, then retries.
    Project::run(second.worktree(), &["merge", "--no-edit", "-q", "main"]).unwrap();
    let landed = second.deliver("B", None).unwrap();
    assert!(landed.commit.is_some());
    assert_eq!(tip(&root), format!("B [{}]", second.handle()));
    assert_eq!(Project::run(&root, &["show", "main:a.txt"]).unwrap(), "a\n");
}

#[test]
fn resume_remakes_a_lost_worktree_and_converges_on_a_standing_delivery() {
    // Retry after crash, both halves: the worktree directory is gone, and the
    // squash already landed while nothing recorded it.
    let (tmp, root, p) = project();
    let x = xdg(&tmp);
    let target = p.target(None).unwrap();
    let handle = {
        let a = Attempt::open(&root, &x, &target).unwrap();
        work(a.worktree(), "c.txt", "c\n");
        a.deliver("Landed", None).unwrap();
        // The crash: worktree directory removed under a live attempt, the
        // source ref surviving.
        fs::remove_dir_all(a.worktree()).unwrap();
        a.handle().to_string()
    };
    let landed = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();

    let again = Attempt::resume(&root, &x, &target, &handle).unwrap();
    assert!(again.worktree().join("c.txt").exists(), "the stale registration healed");
    assert_eq!(again.base(), Project::run(&root, &["merge-base", "main", &attempt_branch(&handle)])
        .unwrap()
        .trim(), "a resumed base is the true fork point, not a re-pin");

    // Converged: no duplicate delivery commit, and the STANDING one is reported.
    let d = again.deliver("Landed", None).unwrap();
    assert_eq!(d.commit, Some(landed.clone()));
    assert_eq!(Project::run(&root, &["rev-parse", "main"]).unwrap().trim(), landed);
}

#[test]
fn a_failing_gate_aborts_before_the_seal_and_leaves_the_target_unmoved() {
    let (tmp, root, p) = project();
    let target = p.target(None).unwrap();
    let a = Attempt::open(&root, &xdg(&tmp), &target).unwrap();
    work(a.worktree(), "d.txt", "d\n");
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let err = a.deliver("Rejected by the gate", None).unwrap_err().to_string();
    assert!(err.contains("delivery gate"), "{err}");
    assert_eq!(tip(&root), "seed", "nothing landed");
    assert!(a.worktree().join("d.txt").exists(), "the source is intact for the fix");
}

#[test]
fn release_keeps_a_rejected_source_addressable_and_discard_removes_it() {
    let (tmp, root, p) = project();
    let x = xdg(&tmp);
    let target = p.target(None).unwrap();
    let a = Attempt::open(&root, &x, &target).unwrap();
    work(a.worktree(), "loser.txt", "loser\n");
    let tip_of_a = a.tip().unwrap();
    let handle = a.handle().to_string();

    // REJECTED: no delivery at all. The target never moved, and the loser stays
    // inspectable — retention is the caller's, separate from worktree release.
    a.release().unwrap();
    assert!(!a.worktree().exists());
    assert_eq!(tip(&root), "seed");
    assert_eq!(a.tip().unwrap(), tip_of_a, "the source ref still resolves");
    assert_eq!(
        Project::run(&root, &["show", &format!("{tip_of_a}:loser.txt")]).unwrap(),
        "loser\n",
        "and its bytes are still readable"
    );
    // A released attempt re-materializes on resume — release is not discard.
    let back = Attempt::resume(&root, &x, &target, &handle).unwrap();
    assert!(back.worktree().join("loser.txt").exists());

    // EXPLICIT discard: worktree and source ref both go, and the handle dies.
    back.discard().unwrap();
    assert!(!back.worktree().exists());
    let err = Attempt::resume(&root, &x, &target, &handle).unwrap_err().to_string();
    assert!(err.contains("unknown attempt handle"), "{err}");
}

#[test]
fn a_bare_project_repo_attempts_and_delivers() {
    let (tmp, root, _) = project();
    let bare = tmp.path().join("bare.git");
    Project::run(tmp.path(), &["clone", "--bare", "-q", root.to_str().unwrap(), bare.to_str().unwrap()]).unwrap();
    Project::run(&bare, &["config", "user.name", "test"]).unwrap();
    Project::run(&bare, &["config", "user.email", "test@example.com"]).unwrap();
    let p = Project::at(&bare);

    let target = p.target(None).unwrap();
    let a = Attempt::open(&bare, &xdg(&tmp), &target).unwrap();
    work(a.worktree(), "bare.txt", "bare\n");
    let d = a.deliver("From a bare repo", None).unwrap();

    assert_eq!(d.target, "main");
    assert!(d.commit.is_some());
    assert_eq!(Project::run(&bare, &["show", "main:bare.txt"]).unwrap(), "bare\n");
}

#[test]
fn a_target_names_the_ball_graph_an_explicit_branch_or_a_parent_attempt() {
    let (tmp, root, p) = project();
    let x = xdg(&tmp);

    // The ball graph: a close-gating child targets `work/<parent>`, lazily
    // minted at the integration head — the same derivation `bl close` makes.
    let ball = p.target(Some("bl-epic")).unwrap();
    assert!(p.branch_exists("work/bl-epic").unwrap());

    // An explicit, validated branch. And its deleted/moved counterpart.
    Project::run(&root, &["branch", "release"]).unwrap();
    p.target_ref("release").unwrap();
    let err = p.target_ref("gone").unwrap_err().to_string();
    assert!(err.contains("no such delivery target: gone"), "{err}");

    // A parent attempt IS a target: a write-capable child delivers into its
    // parent's source ref — the fractal law one depth down, expressed without
    // the caller ever spelling a ref name.
    let parent = Attempt::open(&root, &x, &ball).unwrap();
    let child = Attempt::open(&root, &x, &parent.target()).unwrap();
    work(child.worktree(), "child.txt", "child\n");
    let d = child.deliver("Child work", None).unwrap();
    assert_eq!(d.target, attempt_branch(parent.handle()));
    assert_eq!(tip(&root), "seed", "the integration branch is untouched at this depth");
    assert_eq!(
        Project::run(&root, &["show", &format!("{}:child.txt", attempt_branch(parent.handle()))]).unwrap(),
        "child\n"
    );
}

#[test]
fn the_ball_path_returns_the_same_identities_the_attempt_path_does() {
    // Ordinary claim/close parity: one mechanism, so `work/<id>` reports exactly
    // what `attempt/<handle>` reports.
    let (tmp, root, p) = project();
    let before = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    work(&wt, "ball.txt", "ball\n");

    let d = p.deliver(&wt, "work/bl-x", "main", "Ball work [bl-x]", "[bl-x]").unwrap();

    assert_eq!(d.target, "main");
    assert_eq!(d.base, before);
    assert_eq!(d.source, Some(Project::run(&root, &["rev-parse", "work/bl-x"]).unwrap().trim().to_string()));
    assert_eq!(d.commit, Some(Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string()));
}

#[test]
fn the_no_op_arms_still_report_the_base_and_target_they_acted_on() {
    let (tmp, root, p) = project();
    let before = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();

    // Never authored: no source ref at all.
    let d = p.deliver(&tmp.path().join("absent"), "work/bl-none", "main", "Nothing [bl-none]", "[bl-none]").unwrap();
    assert_eq!((d.target.as_str(), d.base.as_str(), d.source, d.commit), ("main", before.as_str(), None, None));

    // Authored, but the target already carries everything: an empty deliverable.
    let a = Attempt::open(&root, &xdg(&tmp), &p.target(None).unwrap()).unwrap();
    let d = a.deliver("Empty", None).unwrap();
    assert_eq!(d.base, before);
    assert_eq!(d.source, Some(a.tip().unwrap()));
    assert_eq!(d.commit, None, "nothing landed, and the identities still read");
    assert_eq!(tip(&root), "seed");
}

#[test]
fn the_capability_refuses_a_non_repo_root_and_an_unsafe_invocation_path() {
    let tmp = TempDir::new().unwrap();
    let err = Project::at(tmp.path()).target(None).unwrap_err().to_string();
    assert!(err.contains("not a git repository"), "balls' own voice, not git's: {err}");
    let err = Project::at(tmp.path()).target_ref("main").unwrap_err().to_string();
    assert!(err.contains("not a git repository"), "{err}");

    // Absolute but traversing: the worktree mirror joins the path verbatim, so a
    // `..` component would let it escape attempt territory (bl-2d6d).
    let (tmp, root, p) = project();
    let target = p.target(None).unwrap();
    let sneaky = root.parent().unwrap().join("proj/../proj");
    let err = Attempt::open(&sneaky, &xdg(&tmp), &target).unwrap_err().to_string();
    assert!(err.contains("unsafe invocation path"), "{err}");
}
