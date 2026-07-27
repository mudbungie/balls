//! The delivery-message policy (bl-b9a6; subject/body split bl-9961):
//! [`compose`]'s always-tagged-subject / body-narration rules as pure cases,
//! and [`deliver_close`] / [`Repo::work_messages`] end to end on throwaway
//! project repos (the real git read, the merge skip, the squash).

use super::*;

use crate::delivery::{Repo, Spec};
use crate::delivery_repo::tests::{project, tip};
use crate::delivery_repo::Project;
use std::fs;

#[test]
fn compose_is_the_bare_tagged_subject_with_no_narration() {
    // No `-m`, no work commits (empty deliverable) → just the tagged subject.
    assert_eq!(compose(None, &[], "Title [bl-x]"), "Title [bl-x]");
    // Whitespace-only work entries (the NUL-split tail) are not "usable work".
    let blank = vec!["  \n ".to_string()];
    assert_eq!(compose(None, &blank, "Title [bl-x]"), "Title [bl-x]");
}

#[test]
fn compose_puts_work_messages_in_the_body_under_the_tagged_subject() {
    let work = vec!["first\n\nbody".to_string(), "second".to_string()];
    // The subject is ALWAYS the tagged title; work messages are the body,
    // oldest-first, blank-line joined (never displacing the subject).
    assert_eq!(compose(None, &work, "Title [bl-x]"), "Title [bl-x]\n\nfirst\n\nbody\n\nsecond");
}

#[test]
fn compose_leads_the_body_with_the_m_narration_then_keeps_the_work_context() {
    // Both go in the body TOGETHER — `-m` first, then work; neither elects the
    // other out, so bl-b9a6's rich work context survives even with `-m`.
    let work = vec!["work rationale".to_string()];
    assert_eq!(compose(Some("Do it"), &work, "T [bl-x]"), "T [bl-x]\n\nDo it\n\nwork rationale");
    // A whitespace-only `-m` is absent → body is the work context alone.
    assert_eq!(compose(Some("   "), &work, "T [bl-x]"), "T [bl-x]\n\nwork rationale");
}

#[test]
fn compose_never_lets_narration_displace_or_duplicate_the_subject_tag() {
    // A body-only multi-paragraph `-m` (the papercut of bl-9961) is body, NOT
    // the subject — the tag stays clean on the title, no `[id]` mid-sentence.
    assert_eq!(compose(Some("A para\n\nsecond para"), &[], "T [bl-x]"), "T [bl-x]\n\nA para\n\nsecond para");
    // Narration mentioning the tag does not double it on the subject.
    assert_eq!(compose(Some("Done [bl-x] already"), &[], "T [bl-x]"), "T [bl-x]\n\nDone [bl-x] already");
}

#[test]
fn compose_drops_a_part_that_is_just_the_subject_so_retries_do_not_compound() {
    // balls' own capture commit is labelled with the delivery SUBJECT LINE; an
    // aborted close leaves it on work/<id>, so the next close reads it back.
    // It says nothing the subject does not — drop it, and the composition is
    // idempotent across retries (bl-a500).
    let work = vec!["Title [bl-x]".to_string(), "real work".to_string(), "Title [bl-x]".to_string()];
    assert_eq!(compose(None, &work, "Title [bl-x]"), "Title [bl-x]\n\nreal work");
    // Nothing but capture echoes → the bare tagged subject, no stray blank body.
    assert_eq!(compose(None, &work[..1], "Title [bl-x]"), "Title [bl-x]");
}

#[test]
fn compose_caps_the_body_and_says_how_many_it_dropped() {
    // A force-push rewrite under a live work branch makes orphaned upstream
    // history read as authored (no graph bound can tell them apart), so the
    // body must have an end. It is cut on a whole-message boundary.
    let big = "x".repeat(BODY_CAP / 2 - 2); // two fit the budget exactly (joins counted), the third does not
    let work = vec![big.clone(), big.clone(), big.clone()];
    let out = compose(None, &work, "T [bl-x]");
    assert_eq!(out, format!("T [bl-x]\n\n{big}\n\n{big}\n\nand 1 more work commit message(s), over the {BODY_CAP}-byte body budget"));
    // A single part over budget keeps the subject and the honest count — never
    // a truncated half-message.
    let huge = vec!["y".repeat(BODY_CAP + 1)];
    assert_eq!(
        compose(None, &huge, "T [bl-x]"),
        format!("T [bl-x]\n\nand 1 more work commit message(s), over the {BODY_CAP}-byte body budget")
    );
}

#[test]
fn subject_line_is_the_first_line_of_any_message() {
    assert_eq!(subject_line("T [bl-x]\n\nbody\nmore"), "T [bl-x]");
    assert_eq!(subject_line("T [bl-x]"), "T [bl-x]");
    assert_eq!(subject_line(""), ""); // no lines at all → the message itself
}

#[test]
fn work_messages_is_empty_for_a_branch_never_made() {
    let (_tmp, _root, p) = project();
    assert!(p.work_messages("work/bl-absent", "main").unwrap().is_empty());
}

#[test]
fn work_messages_lists_authored_commits_oldest_first_skipping_merges() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("a.txt"), "1\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "first commit\n\nbody one"]).unwrap();
    fs::write(wt.join("b.txt"), "2\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "second commit"]).unwrap();
    // Integration advances and the author folds it in → a merge commit on the
    // branch that work_messages must NOT mistake for authored content.
    fs::write(root.join("c.txt"), "3\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-q", "-m", "main moved"]).unwrap();
    Project::run(&wt, &["merge", "--no-edit", "main"]).unwrap();

    let got: Vec<String> =
        p.work_messages("work/bl-x", "main").unwrap().iter().map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect();
    assert_eq!(got, ["first commit\n\nbody one", "second commit"]);
}

#[test]
fn deliver_close_carries_the_authors_rich_work_body_to_main() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("f.txt"), "x\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    let rich = "Fix the squash message\n\nThe delivery commit used to be only the ball\ntitle; now it carries the body.";
    Project::run(&wt, &["commit", "-q", "-m", rich]).unwrap();

    let spec = Spec {
        worktree: &wt,
        branch: "work/bl-x",
        subject: "ball title [bl-x]",
        override_msg: None,
        marker: "[bl-x]",
        target: None,
    };
    deliver_close(&p, &spec).unwrap();

    // The subject is ALWAYS the tagged ball title; the author's rich work body
    // lands UNDER it — not as the subject — and no "Merge branch 'main'" leaks.
    let body = Project::run(&root, &["log", "-1", "--format=%B", "main"]).unwrap();
    assert_eq!(body.trim(), "ball title [bl-x]\n\nFix the squash message\n\nThe delivery commit used to be only the ball\ntitle; now it carries the body.");
    assert_eq!(tip(&root), "ball title [bl-x]");
}

#[test]
fn deliver_close_builds_the_body_from_branch_own_commits_across_a_folded_merge() {
    // The bl-a500 shape: the agent folded integration in BY HAND after an
    // aborted close, so a merge commit rides `work/<id>`. Neither the fold
    // itself nor anything it brought over from integration is authored work —
    // only the branch's own commits reach the delivery body.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("f.txt"), "x\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "authored one"]).unwrap();
    fs::write(root.join("g.txt"), "y\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-q", "-m", "upstream noise"]).unwrap();
    Project::run(&wt, &["merge", "--no-edit", "main"]).unwrap();
    fs::write(wt.join("f.txt"), "xx\n").unwrap();
    Project::run(&wt, &["commit", "-qam", "authored two"]).unwrap();

    let spec = Spec {
        worktree: &wt,
        branch: "work/bl-x",
        subject: "ball title [bl-x]",
        override_msg: None,
        marker: "[bl-x]",
        target: None,
    };
    deliver_close(&p, &spec).unwrap();

    let body = Project::run(&root, &["log", "-1", "--format=%B", "main"]).unwrap();
    assert_eq!(body.trim(), "ball title [bl-x]\n\nauthored one\n\nauthored two");
}

#[test]
fn deliver_close_keeps_the_m_narration_and_the_work_body_under_the_tagged_subject() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("f.txt"), "x\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "work body kept"]).unwrap();

    let spec = Spec {
        worktree: &wt,
        branch: "work/bl-x",
        subject: "ball title [bl-x]",
        override_msg: Some("Close note\n\nthe full narration"),
        marker: "[bl-x]",
        target: None,
    };
    deliver_close(&p, &spec).unwrap();

    // Subject stays the tagged title; body = the `-m` narration FIRST, then the
    // work context (both survive — bl-9961). tip (the subject) is never the -m.
    let body = Project::run(&root, &["log", "-1", "--format=%B", "main"]).unwrap();
    assert_eq!(body.trim(), "ball title [bl-x]\n\nClose note\n\nthe full narration\n\nwork body kept");
    assert_eq!(tip(&root), "ball title [bl-x]");
}
