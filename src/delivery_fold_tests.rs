//! Fold-rigor tests (bl-a04a): the strict fold (modify/delete conflicts abort;
//! delivery never concludes a half-merge) and the no-resurrection invariant at
//! squash — each on a throwaway project repo shaped like the bl-33db incident.

use crate::delivery::Repo;
use crate::delivery_repo::tests::{project, tip};
use crate::delivery_repo::Project;
use std::fs;

/// `git -C <root> <args>`, unwrapped — the test-side shorthand.
fn g(root: &std::path::Path, args: &[&str]) -> String {
    Project::run(root, args).unwrap()
}

/// The bl-33db shape: the work branch MODIFIED a file a sibling's delivery
/// DELETED on main. Returns (tmp, root, project, worktree).
fn modify_delete_race() -> (tempfile::TempDir, std::path::PathBuf, Project, std::path::PathBuf) {
    let (tmp, root, p) = project();
    fs::write(root.join("doomed.txt"), "original\n").unwrap();
    g(&root, &["add", "-A"]);
    g(&root, &["commit", "-qm", "add doomed"]);
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("doomed.txt"), "work edit\n").unwrap();
    g(&wt, &["commit", "-qam", "work edits doomed"]);
    g(&root, &["rm", "-q", "doomed.txt"]); // the sibling's delivered deletion
    g(&root, &["commit", "-qm", "main deletes doomed"]);
    (tmp, root, p, wt)
}

#[test]
fn a_modify_delete_fold_conflict_aborts_the_close() {
    let (_tmp, root, p, wt) = modify_delete_race();
    let err = p.deliver(&wt, "work/bl-x", "main", "clash [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("delivery conflict"), "{err}");
    assert_eq!(tip(&root), "main deletes doomed"); // integration untouched
    // The half-merge was aborted: the worktree is clean for a hand resolve.
    assert!(!Project::ok(&wt, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]).unwrap());
}

#[test]
fn deliver_never_concludes_a_half_merge_left_in_the_worktree() {
    let (_tmp, root, p, wt) = modify_delete_race();
    // The agent merged by hand, hit the modify/delete conflict, and retried the
    // close without resolving. Capture's add -A + commit would conclude the
    // merge work-side — resurrecting doomed.txt (the bl-33db path).
    assert!(Project::run(&wt, &["merge", "main"]).is_err());
    assert!(Project::ok(&wt, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]).unwrap());

    let err = p.deliver(&wt, "work/bl-x", "main", "retry [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("merge is in progress"), "{err}");
    // Nothing was concluded or delivered: the merge is still the agent's.
    assert!(Project::ok(&wt, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]).unwrap());
    assert_eq!(tip(&root), "main deletes doomed");
}

#[test]
fn the_squash_aborts_naming_a_path_the_work_never_authored() {
    // An evil merge resurrects a file the work branch never touched: the fold
    // itself is clean (work never edited doomed.txt), but the merge commit
    // restores it against main's deletion. No work commit authored doomed.txt,
    // so the invariant must abort the close naming it.
    let (tmp, root, p) = project();
    fs::write(root.join("doomed.txt"), "original\n").unwrap();
    g(&root, &["add", "-A"]);
    g(&root, &["commit", "-qm", "add doomed"]);
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    g(&wt, &["add", "-A"]);
    g(&wt, &["commit", "-qm", "work feature"]);
    g(&root, &["rm", "-q", "doomed.txt"]);
    g(&root, &["commit", "-qm", "main deletes doomed"]);
    g(&wt, &["merge", "--no-commit", "-q", "main"]); // clean — stops pre-commit
    g(&wt, &["checkout", "HEAD", "--", "doomed.txt"]); // the resurrection
    g(&wt, &["commit", "-qm", "fold (evil: restores doomed)"]);

    let err = p.deliver(&wt, "work/bl-x", "main", "res [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("no-resurrection invariant"), "{err}");
    assert!(err.to_string().contains("doomed.txt"), "names the path: {err}");
    assert_eq!(tip(&root), "main deletes doomed"); // integration untouched
}

#[test]
fn a_hand_resolved_fold_counts_as_authored_and_delivers() {
    // Both sides edit conflict.txt; the close-fold aborts; the agent resolves
    // by hand — the resolution differs from BOTH parents and also fixes up
    // extra.txt, a path no non-merge work commit touched. Resolution paths are
    // work commits (the --cc arm), so the retried close delivers.
    let (tmp, root, p) = project();
    fs::write(root.join("conflict.txt"), "base\n").unwrap();
    fs::write(root.join("extra.txt"), "stale\n").unwrap();
    g(&root, &["add", "-A"]);
    g(&root, &["commit", "-qm", "base"]);
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("conflict.txt"), "work\n").unwrap();
    g(&wt, &["commit", "-qam", "work edit"]);
    fs::write(root.join("conflict.txt"), "main\n").unwrap();
    g(&root, &["commit", "-qam", "main edit"]);
    assert!(p.deliver(&wt, "work/bl-x", "main", "r [bl-x]", "[bl-x]").is_err()); // strict fold

    assert!(Project::run(&wt, &["merge", "main"]).is_err()); // the hand resolve
    fs::write(wt.join("conflict.txt"), "merged\n").unwrap();
    fs::write(wt.join("extra.txt"), "fixed in resolution\n").unwrap();
    g(&wt, &["add", "-A"]);
    g(&wt, &["commit", "-qm", "fold resolution"]);

    p.deliver(&wt, "work/bl-x", "main", "r [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "r [bl-x]");
    assert_eq!(g(&root, &["show", "main:conflict.txt"]), "merged\n");
    assert_eq!(g(&root, &["show", "main:extra.txt"]), "fixed in resolution\n");
}
