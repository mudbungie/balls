//! The delivery-message policy (bl-b9a6): [`compose`]'s precedence + tag rules
//! as pure cases, and [`deliver_close`] / [`Repo::work_messages`] end to end on
//! throwaway project repos (the real git read, the merge skip, the squash).

use super::*;

use crate::delivery::{Repo, Spec};
use crate::delivery_repo::tests::{project, tip};
use crate::delivery_repo::Project;
use std::fs;

#[test]
fn compose_falls_back_to_the_tagged_subject_with_no_override_or_work() {
    assert_eq!(compose(None, &[], "Title [bl-x]", "[bl-x]"), "Title [bl-x]");
    // Whitespace-only work entries (the NUL-split tail) are not "usable work".
    let blank = vec!["  \n ".to_string()];
    assert_eq!(compose(None, &blank, "Title [bl-x]", "[bl-x]"), "Title [bl-x]");
}

#[test]
fn compose_joins_multiple_work_messages_oldest_first_and_tags_the_subject() {
    let work = vec!["first\n\nbody".to_string(), "second".to_string()];
    // Blank-line joined; the tag lands on the first subject line, body intact.
    assert_eq!(compose(None, &work, "Title [bl-x]", "[bl-x]"), "first [bl-x]\n\nbody\n\nsecond");
}

#[test]
fn compose_override_wins_and_an_empty_override_is_ignored() {
    assert_eq!(compose(Some("Do it"), &["work".to_string()], "T [bl-x]", "[bl-x]"), "Do it [bl-x]");
    // A whitespace-only `-m` is treated as absent → the work message is used.
    assert_eq!(compose(Some("   "), &["work".to_string()], "T [bl-x]", "[bl-x]"), "work [bl-x]");
}

#[test]
fn with_marker_is_idempotent_and_tags_a_lone_subject_line() {
    // Already carrying the tag (anywhere) → left untouched, no `[id] [id]`.
    assert_eq!(compose(Some("Done [bl-x] already"), &[], "T [bl-x]", "[bl-x]"), "Done [bl-x] already");
    // A single-line message with no body gets the tag appended.
    assert_eq!(compose(Some("Just a subject"), &[], "T [bl-x]", "[bl-x]"), "Just a subject [bl-x]");
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
    };
    deliver_close(&p, &spec).unwrap();

    // The delivered commit is the author's body, with the tag on the subject —
    // not the bare "ball title", and no "Merge branch 'main'" anywhere.
    let body = Project::run(&root, &["log", "-1", "--format=%B", "main"]).unwrap();
    assert_eq!(body.trim(), "Fix the squash message [bl-x]\n\nThe delivery commit used to be only the ball\ntitle; now it carries the body.");
    assert_eq!(tip(&root), "Fix the squash message [bl-x]");
}

#[test]
fn deliver_close_honors_an_m_override_over_the_work_body() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("f.txt"), "x\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "work body that loses"]).unwrap();

    let spec = Spec {
        worktree: &wt,
        branch: "work/bl-x",
        subject: "ball title [bl-x]",
        override_msg: Some("Override wins\n\nthe full message"),
        marker: "[bl-x]",
    };
    deliver_close(&p, &spec).unwrap();

    let body = Project::run(&root, &["log", "-1", "--format=%B", "main"]).unwrap();
    assert_eq!(body.trim(), "Override wins [bl-x]\n\nthe full message");
}
