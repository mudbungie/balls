//! [`Project`] DELIVERY tests — the direct local-squash and its convergence:
//! capturing pending work, the empty-deliverable / never-made-branch no-ops, a
//! surfaced conflict, the two bl-430e retry-idempotence shapes (already-landed
//! and already-merged), and `marked`'s newest-first incarnation scan. Shares the
//! `project`/`tip` fixtures with the sibling act tests via [`super::tests`].

use super::tests::{project, tip};
use super::*;
use crate::delivery::Repo;
use std::fs;

#[test]
fn deliver_captures_pending_work_and_squashes_it_onto_integration() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    // Uncommitted work in the code worktree — deliver must capture it.
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();

    assert_eq!(tip(&root), "Add feature [bl-x]");
    // The squash landed as ONE commit on main, parented on the seed.
    assert_eq!(Project::run(&root, &["show", "main:feature.txt"]).unwrap(), "shipped\n");
    assert_eq!(Project::run(&root, &["rev-list", "--count", "main"]).unwrap().trim(), "2");
}

#[test]
fn deliver_with_no_pending_work_but_a_committed_branch_still_squashes() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("f.txt"), "x\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "wip"]).unwrap();

    // Nothing pending now (already committed) — capture is a clean no-op, the
    // squash still delivers the committed branch state.
    p.deliver(&wt, "work/bl-x", "main", "Land it [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "Land it [bl-x]");
}

#[test]
fn a_message_past_the_argv_limit_delivers_via_stdin_and_never_labels_the_capture() {
    // `MAX_ARG_STRLEN` is 128 KiB on Linux: spelled as `commit-tree -m`, this
    // message killed the delivery with a bare `Argument list too long (os error
    // 7)` — after the 10-minute gate, with nothing landed (bl-a500). Every
    // message channel is stdin now; argv carries only the subject LINE.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap(); // dirty → capture runs too
    let message = format!("Add feature [bl-x]\n\n{}", "context line\n".repeat(20_000)); // ~260 KB

    p.deliver(&wt, "work/bl-x", "main", &message, "[bl-x]").unwrap();

    // The whole message rode stdin into commit-tree and landed verbatim.
    let landed = Project::run(&root, &["log", "-1", "--format=%B", "main"]).unwrap();
    assert_eq!(landed.trim_end(), message.trim_end());
    assert_eq!(tip(&root), "Add feature [bl-x]");
    // The reflog (an inherently one-line record) got the subject line, not the message.
    let reflog = Project::run(&root, &["reflog", "show", "--format=%gs", "-1", "main"]).unwrap();
    assert_eq!(reflog.trim(), "Add feature [bl-x]");
    // And so did the capture commit — labelling it with the composed message is
    // what compounded it per aborted close until it blew the limit.
    let capture = Project::run(&root, &["log", "--no-merges", "-1", "--format=%B", "work/bl-x"]).unwrap();
    assert_eq!(capture.trim_end(), "Add feature [bl-x]");
}

#[test]
fn deliver_is_a_no_op_for_an_empty_deliverable() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap(); // claimed, never worked → no diff

    p.deliver(&wt, "work/bl-x", "main", "nothing [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "seed"); // integration untouched
}

#[test]
fn deliver_is_a_no_op_when_the_branch_was_never_made() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt"); // never materialized
    p.deliver(&wt, "work/bl-z", "main", "nothing [bl-z]", "[bl-z]").unwrap();
    assert_eq!(tip(&root), "seed");
}

#[test]
fn deliver_surfaces_a_conflict_as_an_error() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("seed.txt"), "from work\n").unwrap();
    Project::run(&wt, &["commit", "-qam", "work edit"]).unwrap();
    // Integration moves the same line — the squash can't merge cleanly.
    fs::write(root.join("seed.txt"), "from main\n").unwrap();
    Project::run(&root, &["commit", "-qam", "main edit"]).unwrap();

    let err = p.deliver(&wt, "work/bl-x", "main", "clash [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("delivery conflict"));
    // The half-merge was aborted: no MERGE_HEAD pending, the worktree is clean
    // for the agent to reintegrate by hand.
    assert!(!Project::ok(&wt, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]).unwrap());
    assert!(Project::ok(&wt, &["diff", "--quiet", "HEAD"]).unwrap());
}

#[test]
fn deliver_skips_when_this_incarnations_delivery_already_landed() {
    // The bl-430e retry: a close squash-delivered, then aborted after the seal
    // (push race) — the squash is BINDING and stands (§14); main keeps the
    // delivery and the branch survives. The re-close must not mint a duplicate.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    // Commit under a DIFFERENT subject so the squash deterministically mints a
    // distinct sha (capture + squash of the same tree/parent/message in the
    // same second collide — the is-ancestor guard's case, tested below).
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    // A concurrent agent lands on main AFTER the delivery, so main and the
    // branch differ again — the empty-deliverable guard alone would re-squash,
    // and `merge-tree` of already-merged work yields main's own tree: an EMPTY
    // duplicate delivery commit (the bl-3bfd outcome).
    fs::write(root.join("other.txt"), "other\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent work"]).unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(p.marked("main", "[bl-x]").unwrap().len(), 1); // one delivery, no dup
    assert_eq!(tip(&root), "concurrent work"); // the retry minted nothing
}

#[test]
fn deliver_skips_a_branch_already_fully_merged_into_integration() {
    // The sha-collision shape of the bl-430e retry: capture then squash can mint
    // the SAME commit (same parent/tree/message/second), so the surviving
    // delivery IS the branch tip. Every branch commit on integration = nothing
    // to deliver, even once integration moves on (trees differ again).
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "Add feature [bl-x]"]).unwrap();
    Project::run(&root, &["merge", "-q", "--ff-only", "work/bl-x"]).unwrap(); // the collided delivery
    fs::write(root.join("other.txt"), "other\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent work"]).unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(p.marked("main", "[bl-x]").unwrap().len(), 1);
    assert_eq!(tip(&root), "concurrent work");
}

#[test]
fn commit_swap_aborts_when_integration_moved_under_the_delivery() {
    // The bl-a3bb race arm: a sibling close lands on `main` in the window
    // between the parent read and the ref move, so the pre-read parent is now
    // stale. The CAS must refuse the write (loud abort) rather than clobber it.
    let (_tmp, root, _p) = project();
    let stale = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string(); // the seed
    fs::write(root.join("other.txt"), "x\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent"]).unwrap(); // main moved past `stale`
    let tip_sha = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();

    // A CAS carrying the STALE parent is rejected — git keeps main where the
    // concurrent close left it and nothing is overwritten.
    let err = super::acts::commit_swap(&root, "main", "late [bl-x]", &tip_sha, &stale).unwrap_err();
    assert!(err.to_string().contains("moved under the delivery"));
    assert_eq!(tip(&root), "concurrent");
}

#[test]
fn marked_returns_the_marked_commits_newest_first() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");

    // First incarnation of bl-x: deliver onto main, then close it out (discard).
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("a.txt"), "1\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "first [bl-x]", "[bl-x]").unwrap();
    p.discard(&wt, "work/bl-x").unwrap();

    // A reused id only begins after the prior closed, so its delivery lands
    // LATER — deliveries are monotonic with incarnations (§11). The second
    // deliver MUST land despite the first `[bl-x]` in history: the
    // retry-idempotence skip (bl-430e) is scoped to commits since this
    // branch forked, and the prior delivery predates the fork.
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("b.txt"), "2\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "second [bl-x]", "[bl-x]").unwrap();

    let shas = p.marked("main", "[bl-x]").unwrap();
    let subject = |sha: &str| Project::run(&root, &["log", "-1", "--format=%s", sha]).unwrap().trim().to_string();
    // Newest first: the k-th-most-recent incarnation maps to the k-th element.
    assert_eq!(shas.iter().map(|s| subject(s)).collect::<Vec<_>>(), ["second [bl-x]", "first [bl-x]"]);
    // A never-delivered id → empty (an honest cross-clone miss, §11).
    assert!(p.marked("main", "[bl-zzzz]").unwrap().is_empty());
}
