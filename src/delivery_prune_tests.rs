//! [`Project::prune`] tests — the §11/§14 deferred `work/<id>` branch cleanup
//! on throwaway project repos: delete the settled (delivered / forkless),
//! preserve the undelivered, refuse nothing a live worktree holds. Plus the
//! bl-c117 debris report riding the same pass: silent when the worktree formula
//! resolves to a present directory, one line by value when it does not — in the
//! arm the store decides (bl-baa0: open ball ⇒ re-claim or discard; closed ball
//! ⇒ discard only, qualified by content-containment against `main`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::delivery::Repo;
use crate::delivery_path::worktree_path;
use crate::delivery_repo::tests::{project, tip};
use crate::delivery_repo::Project;
use crate::layout::Xdg;

/// A throwaway XDG base + plugin name, and the exact worktree path `claim`
/// would derive for `id` against `root` — the formula [`Project::prune`]'s
/// debris check reads through ([`worktree_path`]), so a test that materializes
/// there (not at an arbitrary tempdir path, unlike the sibling prune tests
/// above which never exercise the debris probe) proves the real join, not a
/// stand-in.
fn xdg_and_worktree(root: &Path, id: &str) -> (Xdg, &'static str, PathBuf) {
    let xdg = Xdg::with(root, None, Some(&root.join("state").to_string_lossy()));
    let plugin = "bl-delivery";
    let wt = worktree_path(&xdg, plugin, &root.to_string_lossy(), id);
    (xdg, plugin, wt)
}

/// A store checkout under `tmp` holding an OPEN ball per id in `ids` — what
/// core runs `prime.post` in (§13 diffless), and what [`Project::prune`] reads
/// its debris arm from. An id absent from it is CLOSED (§10: absence IS the
/// record), which is exactly what the closed-ball arm keys on.
fn store(tmp: &Path, ids: &[&str]) -> PathBuf {
    let store = tmp.join("store");
    fs::create_dir_all(store.join("tasks")).unwrap();
    for id in ids {
        fs::write(store.join("tasks").join(format!("{id}.md")), "+++\n+++\n").unwrap();
    }
    store
}

#[test]
fn prune_deletes_a_delivered_branch_after_teardown_and_reruns_clean() {
    // The leak this fixes (bl-292d): close delivered + released the worktree
    // but the branch stayed — prune is the deferred cleanup that removes it.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-x");
    let store = store(tmp.path(), &["bl-x"]);
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    p.release(&wt).unwrap(); // close.post teardown: worktree gone, branch left
    // Integration moves on, so the branch tree differs from main's tip — only
    // the fork-scoped [bl-x] delivery scan can call it settled.
    fs::write(root.join("other.txt"), "other\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent work"]).unwrap();

    assert!(p.prune(&xdg, plugin, &store).unwrap().is_empty()); // settled — pruned, never reported
    assert!(!Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-x"]).unwrap());
    assert_eq!(tip(&root), "concurrent work"); // prune touches no integration commit

    assert!(p.prune(&xdg, plugin, &store).unwrap().is_empty()); // idempotent: the pruned branch no longer enumerates
}

#[test]
fn prune_deletes_a_forkless_branch_the_unclaim_of_uncommitted_work_left() {
    // unclaim released the worktree (uncommitted edits discarded with it); the
    // branch carries no commit beyond its fork — deleting it loses nothing.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-y");
    let store = store(tmp.path(), &["bl-y"]);
    p.materialize(&wt, "work/bl-y").unwrap();
    p.release(&wt).unwrap(); // unclaim.post

    assert!(p.prune(&xdg, plugin, &store).unwrap().is_empty());
    assert!(!Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-y"]).unwrap());
}

#[test]
fn prune_preserves_a_branch_carrying_undelivered_commits_and_reports_its_absent_worktree() {
    // The bl-65e0 unclaim contract: work COMMITTED on work/<id> survives — a
    // later claim reattaches the branch and a later close DELIVERS it. Its
    // worktree is gone (materialized at an arbitrary tempdir path, not the
    // claim formula), so the debris report fires by value — in the OPEN arm,
    // the store still holding tasks/bl-z.md.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-z");
    let store = store(tmp.path(), &["bl-z"]);
    p.materialize(&wt, "work/bl-z").unwrap();
    fs::write(wt.join("kept.txt"), "undelivered\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.release(&wt).unwrap(); // unclaim.post: worktree gone, committed work only on the branch

    let reports = p.prune(&xdg, plugin, &store).unwrap();
    assert_eq!(
        reports,
        vec![
            "work/bl-z is committed but its worktree is gone — bl claim bl-z \
             re-materializes onto it (a later close still delivers, bl-65e0), \
             or discard with git branch -D work/bl-z"
        ]
    );
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-z"]).unwrap());

    // And the survival pays off: reclaim + close delivers the preserved work.
    p.materialize(&wt, "work/bl-z").unwrap();
    p.deliver(&wt, "work/bl-z", "main", "Land it [bl-z]", "[bl-z]").unwrap();
    assert_eq!(tip(&root), "Land it [bl-z]");
}

#[test]
fn prune_stays_silent_on_an_undelivered_branch_whose_worktree_is_present() {
    // The debris report is specifically about an ABSENT worktree — a branch
    // still carrying its live worktree at the exact claim-derived path must
    // not be reported (nothing is debris; the claim is simply in progress).
    let (tmp, root, p) = project();
    let (xdg, plugin, wt) = xdg_and_worktree(&root, "bl-live-wt");
    let store = store(tmp.path(), &["bl-live-wt"]);
    p.materialize(&wt, "work/bl-live-wt").unwrap();
    fs::write(wt.join("kept.txt"), "undelivered\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();

    assert!(p.prune(&xdg, plugin, &store).unwrap().is_empty());
}

#[test]
fn prune_leaves_a_checked_out_branch_and_still_succeeds() {
    // A live claim's branch is settled (fresh fork, no commits) but checked out
    // in its worktree — git refuses the delete, and prune, being best-effort,
    // keeps going instead of failing the prime.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-live");
    let store = store(tmp.path(), &["bl-live"]);
    p.materialize(&wt, "work/bl-live").unwrap();

    assert!(p.prune(&xdg, plugin, &store).unwrap().is_empty()); // settled — no report
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-live"]).unwrap());
    assert!(wt.join("seed.txt").exists()); // the worktree is untouched
}

#[test]
fn prune_on_a_root_that_is_no_repo_is_a_quiet_no_op() {
    // A pre-claim prime in a project with no git repo yet: nothing to prune,
    // nothing to fail (the prune is best-effort end to end), nothing to report.
    let outside = tempfile::TempDir::new().unwrap();
    let (xdg, plugin, _) = xdg_and_worktree(outside.path(), "bl-none");
    let store = store(outside.path(), &[]);
    assert!(Project::at(outside.path()).prune(&xdg, plugin, &store).unwrap().is_empty());
}

#[test]
fn prune_scopes_the_delivery_scan_to_the_fork_so_a_reused_id_survives() {
    // A prior incarnation's [bl-r] delivery is an ancestor of the new branch's
    // fork point (§11 recency) — it must not call the NEW incarnation's
    // undelivered work settled.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-r");
    let store = store(tmp.path(), &["bl-r"]);
    p.materialize(&wt, "work/bl-r").unwrap();
    fs::write(wt.join("a.txt"), "1\n").unwrap();
    p.deliver(&wt, "work/bl-r", "main", "first [bl-r]", "[bl-r]").unwrap();
    p.discard(&wt, "work/bl-r").unwrap(); // first incarnation fully torn down

    // Second incarnation forks AFTER the first delivery and commits new work.
    p.materialize(&wt, "work/bl-r").unwrap();
    fs::write(wt.join("b.txt"), "2\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "second wip"]).unwrap();
    p.release(&wt).unwrap();

    let reports = p.prune(&xdg, plugin, &store).unwrap();
    assert_eq!(reports.len(), 1); // undelivered since the fork, worktree gone — reported, not pruned
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-r"]).unwrap());
}

#[test]
fn prune_reports_a_closed_balls_debris_as_discard_only_once_its_content_landed() {
    // bl-baa0, the observed bug: bl-04eb's content landed inside ANOTHER ball's
    // squash, so no [bl-04eb] delivery exists and Standing reads Undelivered
    // forever — yet the ball is CLOSED and its content is fully on main. The
    // old line advised re-claiming a ball that no longer exists. Now the store
    // decides the arm, and content-containment against the integration tip says
    // the deletion is lossless.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-c");
    let store = store(tmp.path(), &[]); // no tasks/bl-c.md — the ball is closed
    p.materialize(&wt, "work/bl-c").unwrap();
    fs::write(wt.join("landed.txt"), "content\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.release(&wt).unwrap();
    // The SAME content lands on main under a sibling ball's tag — untagged for
    // [bl-c], so the fork-scoped marker scan finds nothing.
    fs::write(root.join("landed.txt"), "content\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "Land it [bl-sibling]"]).unwrap();

    assert_eq!(
        p.prune(&xdg, plugin, &store).unwrap(),
        vec![
            "work/bl-c is committed but its worktree is gone, and bl-c is closed \
             (no task file — absence is the record), so nothing can re-claim or \
             deliver it: its content is already contained in main, so discard it \
             with git branch -D work/bl-c"
        ]
    );
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-c"]).unwrap());
}

#[test]
fn prune_hands_a_closed_balls_uncontained_debris_the_diff_to_read_first() {
    // The other closed-ball fate: nothing of the branch is on main, so deleting
    // it DOES lose content. Discard is still the only remedy (no claim, no
    // close), but the line refuses to call it safe — it names the three-dot
    // diff to read first instead of asserting either way.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    let (xdg, plugin, _) = xdg_and_worktree(&root, "bl-d");
    let store = store(tmp.path(), &[]);
    p.materialize(&wt, "work/bl-d").unwrap();
    fs::write(wt.join("orphan.txt"), "only here\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.release(&wt).unwrap();

    assert_eq!(
        p.prune(&xdg, plugin, &store).unwrap(),
        vec![
            "work/bl-d is committed but its worktree is gone, and bl-d is closed \
             (no task file — absence is the record), so nothing can re-claim or \
             deliver it: its content is NOT contained in main — read it with \
             git diff main...work/bl-d, then discard with git branch -D work/bl-d"
        ]
    );
}
