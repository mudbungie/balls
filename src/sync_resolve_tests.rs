//! Unit coverage for `auto_resolve_task_conflicts`. The notes-merge
//! branch is auto-add (notes are append-only — `*.notes.jsonl merge=union`
//! drops conflict markers anyway), and the task-json branch routes
//! through `resolve::resolve_conflict`. Both branches share the same
//! conflict-listing scan, so the test exercises them in a single
//! conflict-bearing repo.

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn git_run(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {} failed", args.join(" "));
}

/// Set up a tiny git repo with a manufactured merge conflict on a
/// `.balls/tasks/<id>.json` file and a `.notes.jsonl` sibling, so the
/// conflict-listing scan returns both shapes.
fn repo_with_conflict() -> (TempDir, PathBuf) {
    let td = TempDir::new().unwrap();
    let root = td.path().to_path_buf();
    git_run(&root, &["init", "-q", "-b", "main"]);
    git_run(&root, &["config", "user.email", "t@t"]);
    git_run(&root, &["config", "user.name", "t"]);
    git_run(&root, &["config", "commit.gpgsign", "false"]);
    let tasks = root.join(".balls/tasks");
    fs::create_dir_all(&tasks).unwrap();
    // Seed both files on main.
    let id_path = tasks.join("bl-aaaa.json");
    let notes_path = tasks.join("bl-aaaa.notes.jsonl");
    let base = serde_json::json!({
        "id": "bl-aaaa", "title": "base", "type": "task", "priority": 3,
        "status": "open", "parent": null, "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z", "closed_at": null,
        "claimed_by": null, "branch": null
    });
    fs::write(&id_path, serde_json::to_string(&base).unwrap()).unwrap();
    fs::write(&notes_path, "base-note\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-qm", "seed"]);

    // Branch off and diverge.
    git_run(&root, &["checkout", "-q", "-b", "side"]);
    let mut side = base.clone();
    side["title"] = serde_json::json!("side-title");
    fs::write(&id_path, serde_json::to_string(&side).unwrap()).unwrap();
    fs::write(&notes_path, "side-note\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-qm", "side"]);

    git_run(&root, &["checkout", "-q", "main"]);
    let mut ours = base;
    ours["title"] = serde_json::json!("main-title");
    fs::write(&id_path, serde_json::to_string(&ours).unwrap()).unwrap();
    fs::write(&notes_path, "main-note\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-qm", "main"]);

    // Merge side into main — produces conflicts on both files.
    let out = std::process::Command::new("git")
        .current_dir(&root)
        .args(["merge", "--no-commit", "--no-ff", "side"])
        .output()
        .unwrap();
    // git merge exits non-zero on conflict, which is the state we want.
    let _ = out;
    (td, root)
}

#[test]
fn auto_resolve_handles_notes_and_task_conflicts() {
    let (_td, root) = repo_with_conflict();
    auto_resolve_task_conflicts(&root).unwrap();
    // Both files should be staged now.
    let staged = std::process::Command::new("git")
        .current_dir(&root)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&staged.stdout);
    assert!(s.contains("bl-aaaa.json"), "task json staged: {s}");
    assert!(s.contains("bl-aaaa.notes.jsonl"), "notes staged: {s}");
}

#[test]
fn auto_resolve_errors_on_unhandled_conflict() {
    // A conflict on a non-task path is rejected loudly — the helper
    // refuses to silently merge code/config files.
    let td = TempDir::new().unwrap();
    let root = td.path().to_path_buf();
    git_run(&root, &["init", "-q", "-b", "main"]);
    git_run(&root, &["config", "user.email", "t@t"]);
    git_run(&root, &["config", "user.name", "t"]);
    git_run(&root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("README"), "base\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-qm", "seed"]);

    git_run(&root, &["checkout", "-q", "-b", "side"]);
    fs::write(root.join("README"), "side\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-qm", "side"]);

    git_run(&root, &["checkout", "-q", "main"]);
    fs::write(root.join("README"), "main\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-qm", "main"]);

    let _ = std::process::Command::new("git")
        .current_dir(&root)
        .args(["merge", "--no-commit", "--no-ff", "side"])
        .output();

    let err = auto_resolve_task_conflicts(&root).unwrap_err();
    assert!(format!("{err}").contains("unhandled conflict"), "got: {err}");
}
