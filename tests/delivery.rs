//! SPEC §6 conformance: delivery-link resolution, hint persistence,
//! and self-healing via the `[bl-xxxx]` tag on main.

mod common;

use common::*;

fn claim_review_close(repo: &std::path::Path, id: &str) {
    bl_as(repo, "alice")
        .args(["claim", id])
        .assert()
        .success();
    let wt = repo.join(".balls-worktrees").join(id);
    std::fs::write(wt.join(format!("{}.txt", id)), "body").unwrap();
    bl(repo)
        .args(["review", id, "-m", "ready"])
        .assert()
        .success();
    bl(repo)
        .args(["close", id, "-m", "approved"])
        .assert()
        .success();
}

#[test]
fn review_writes_delivered_in_hint_to_task_file() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "deliverable");

    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("f.txt"), "x").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    let sha = task["delivered_in"].as_str().expect("delivered_in set");
    // The hint must point at a real commit on main, and that commit's
    // subject must carry the task's delivery tag.
    let main_head = git(repo.path(), &["rev-parse", "main"]);
    assert_eq!(main_head.trim(), sha);
    let subject = git(repo.path(), &["log", "-1", "--format=%s", sha]);
    assert!(
        subject.contains(&format!("[{}]", id)),
        "commit subject missing delivery tag: {}",
        subject
    );
}

#[test]
fn show_displays_delivery_after_review() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "showable");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "a").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "done"])
        .assert()
        .success();

    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("delivered:"),
        "bl show should display delivery line: {}",
        stdout
    );
    assert!(
        stdout.contains(&format!("[{}]", id)),
        "delivery line should include the tag: {}",
        stdout
    );
}

#[test]
fn delivered_in_survives_rebase_of_main_via_tag_fallback() {
    // After reviewing, the delivered_in hint points at a specific SHA.
    // Rebasing main (squashing the delivery commit into a new commit
    // with the same tag) changes the SHA — but the fallback tag scan
    // still resolves to the new commit. bl show must display it and
    // mark the hint as stale.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "rebase-me");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "a").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "done"])
        .assert()
        .success();

    let before = git(repo.path(), &["rev-parse", "main"])
        .trim()
        .to_string();

    // Rewrite main's tip: amend the last commit to change its SHA
    // while keeping the tag in the subject.
    git(
        repo.path(),
        &[
            "commit",
            "--amend",
            "-m",
            &format!("rewritten subject [{}]", id),
            "--no-verify",
        ],
    );
    let after = git(repo.path(), &["rev-parse", "main"])
        .trim()
        .to_string();
    assert_ne!(before, after, "amend should change the SHA");

    let json_out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let j: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).unwrap();
    assert_eq!(j["delivered_in_resolved"].as_str().unwrap(), after);
    assert_eq!(j["delivered_in_hint_stale"], true);
}

#[test]
fn delivered_in_returns_none_after_main_reset_past_tag() {
    // Hard-reset main past the delivery commit. The tag is no longer
    // reachable from main → resolution returns None.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "reset-past");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "a").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "done"])
        .assert()
        .success();
    bl(repo.path())
        .args(["close", &id, "-m", "ok"])
        .assert()
        .success();

    // Reset main past the feature commit — the delivery tag is gone
    // from reachable history.
    git(repo.path(), &["reset", "--hard", "HEAD~1"]);

    // bl show needs a resurrected task file to read, since close
    // archived it. Restore the archive-pre state from the state
    // branch's parent of the close commit.
    let state_parent = git(
        repo.path(),
        &["rev-parse", "balls/tasks~1"],
    )
    .trim()
    .to_string();
    let content = git(
        repo.path(),
        &[
            "show",
            &format!("{}:.balls/tasks/{}.json", state_parent, id),
        ],
    );
    std::fs::write(
        repo.path().join(".balls/tasks").join(format!("{}.json", id)),
        content,
    )
    .unwrap();

    let json_out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let j: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).unwrap();
    assert!(j["delivered_in_resolved"].is_null());
    assert_eq!(j["delivered_in_hint_stale"], true);
}

#[test]
fn task_never_reviewed_has_no_delivery_link() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "draft only");

    let task = read_task_json(repo.path(), &id);
    assert!(
        task["delivered_in"].is_null(),
        "freshly-created tasks must not carry a delivery hint"
    );

    let json_out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let j: serde_json::Value =
        serde_json::from_slice(&json_out.stdout).unwrap();
    assert!(j["delivered_in_resolved"].is_null());
    assert_eq!(j["delivered_in_hint_stale"], false);

    let text_out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let stdout = String::from_utf8_lossy(&text_out.stdout);
    assert!(
        !stdout.contains("delivered:"),
        "bl show should omit delivery line for unreviewed tasks: {}",
        stdout
    );
}

#[test]
fn full_review_close_cycle_persists_delivery_on_state_branch() {
    // The hint written during review survives the close flow: after
    // close, the state branch's `state: close` commit has propagated,
    // and a subsequent state-branch tree read still shows the right
    // delivered_in for the archived task.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "cycle");
    claim_review_close(repo.path(), &id);

    // After close, the task file is gone from the state branch's tip.
    // Check its archived state via the parent commit.
    let state_head = git(repo.path(), &["rev-parse", "balls/tasks"])
        .trim()
        .to_string();
    let parent = git(repo.path(), &["rev-parse", &format!("{}~1", state_head)])
        .trim()
        .to_string();
    let archived = git(
        repo.path(),
        &[
            "show",
            &format!("{}:.balls/tasks/{}.json", parent, id),
        ],
    );
    let j: serde_json::Value = serde_json::from_str(&archived).unwrap();
    let sha = j["delivered_in"]
        .as_str()
        .expect("archived task carries delivered_in");
    let subject = git(repo.path(), &["log", "-1", "--format=%s", sha]);
    assert!(subject.contains(&format!("[{}]", id)));
}
