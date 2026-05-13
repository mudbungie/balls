//! `bl review -m` builds a 50/72-shaped squash commit: a short title
//! line with the `[bl-id]` delivery tag, a blank line, then the body.
//! Covers both ways to supply a body — one multi-line `-m` value, and
//! repeated `-m` flags (`git commit -m … -m …` style).

mod common;

use common::*;

#[test]
fn review_squash_commit_uses_50_72_shape_with_body() {
    // A multi-line -m must produce a commit with a short title line
    // (+ [bl-id] tag) and the remainder of the message as a proper
    // body paragraph separated by a blank line.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "structured commit");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "body").unwrap();

    let body = "The title above is short. This paragraph explains \
                the change in detail and wraps across multiple \
                sentences without polluting the oneline log.";
    bl(repo.path())
        .args(["review", &id, "-m", &format!("Short title\n\n{body}")])
        .assert()
        .success();

    // Subject (first line) only contains "Short title [bl-id]".
    let subject = git(repo.path(), &["log", "-1", "--format=%s", "main"]);
    let expected_subject = format!("Short title [{id}]");
    assert_eq!(subject.trim(), expected_subject);

    // Body (after the first blank line) carries the paragraph.
    let full_body = git(repo.path(), &["log", "-1", "--format=%b", "main"]);
    assert!(
        full_body.contains(body),
        "body should be present in full commit message: {full_body}"
    );
}

#[test]
fn review_repeated_m_builds_50_72_body_without_heredoc() {
    // `bl review -m TITLE -m PARA -m PARA` mirrors `git commit -m … -m …`:
    // first value is the subject, each later one is a body paragraph —
    // no shell heredoc needed to get a structured commit message.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "structured commit");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "body").unwrap();

    bl(repo.path())
        .args([
            "review",
            &id,
            "-m",
            "Short title",
            "-m",
            "First paragraph of the body.",
            "-m",
            "Second paragraph of the body.",
        ])
        .assert()
        .success();

    let subject = git(repo.path(), &["log", "-1", "--format=%s", "main"]);
    assert_eq!(subject.trim(), format!("Short title [{id}]"));
    let body = git(repo.path(), &["log", "-1", "--format=%b", "main"]);
    assert!(body.contains("First paragraph of the body."), "body: {body}");
    assert!(body.contains("Second paragraph of the body."), "body: {body}");
}
