//! Schema invariant tests for the text-mergeable task file format.
//!
//! These exercise the property stated in docs/SPEC-orphan-branch-state.md §5:
//! concurrent edits to disjoint fields of the same task must merge cleanly
//! under stock `git merge`, and concurrent note appends must merge cleanly
//! because notes live in an append-only sibling file. Concurrent edits to
//! the *same* field must surface as a genuine conflict.

mod common;

use balls::task::{NewTaskOpts, Task};
use balls::task_io;
use common::{git, git_ok, new_repo};
use std::fs;
use std::path::Path;

/// Build a fresh task, save it to `path`, and commit to the current branch.
/// Pre-seeds an empty notes sidecar and the union-merge `.gitattributes`
/// rule so concurrent appends on divergent branches merge cleanly.
fn seed_task(repo: &Path, id: &str) {
    let t = Task::new(
        NewTaskOpts {
            title: "seed".into(),
            ..Default::default()
        },
        id.into(),
    );
    let tasks_dir = repo.join(".balls/tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::write(tasks_dir.join(".gitattributes"), "*.notes.jsonl merge=union\n").unwrap();
    let task_path = tasks_dir.join(format!("{}.json", id));
    t.save(&task_path).unwrap();
    // Pre-seed an empty notes file so divergent first-append operations
    // modify an existing file rather than each creating a new one.
    fs::write(tasks_dir.join(format!("{}.notes.jsonl", id)), "").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "seed task"]);
}

/// Branch off `main`, mutate the on-disk task with `f`, commit, return to main.
fn branch_and_commit<F: FnOnce(&Task) -> Task>(
    repo: &Path,
    branch: &str,
    id: &str,
    msg: &str,
    f: F,
) {
    git(repo, &["checkout", "-q", "-b", branch]);
    let task_path = repo.join(".balls/tasks").join(format!("{}.json", id));
    let t = Task::load(&task_path).unwrap();
    let updated = f(&t);
    updated.save(&task_path).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", msg]);
    git(repo, &["checkout", "-q", "main"]);
}

/// Branch off `main`, run a raw mutation against files in `.balls/tasks/`,
/// commit, return to main.
fn branch_and_raw<F: FnOnce(&Path)>(repo: &Path, branch: &str, msg: &str, f: F) {
    git(repo, &["checkout", "-q", "-b", branch]);
    let tasks = repo.join(".balls/tasks");
    f(&tasks);
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", msg]);
    git(repo, &["checkout", "-q", "main"]);
}

#[test]
fn disjoint_field_edits_merge_cleanly() {
    let repo = new_repo();
    let root = repo.path();
    // Initial commit so branches have a base.
    git(root, &["commit", "--allow-empty", "-q", "-m", "init"]);
    seed_task(root, "bl-aaaa");

    // Worker A changes priority.
    branch_and_commit(root, "worker-a", "bl-aaaa", "A: bump priority", |t| {
        let mut u = t.clone();
        u.priority = 1;
        u
    });

    // Worker B changes tags (a completely different top-level field).
    branch_and_commit(root, "worker-b", "bl-aaaa", "B: add tag", |t| {
        let mut u = t.clone();
        u.tags.push("urgent".into());
        u
    });

    git(root, &["merge", "-q", "--no-edit", "worker-a"]);
    let merged = git_ok(root, &["merge", "--no-edit", "worker-b"]);
    assert!(
        merged,
        "disjoint-field merge must succeed under stock git merge"
    );

    let final_task = Task::load(
        &root.join(".balls/tasks/bl-aaaa.json"),
    )
    .unwrap();
    assert_eq!(final_task.priority, 1, "worker A's change preserved");
    assert!(
        final_task.tags.contains(&"urgent".to_string()),
        "worker B's change preserved"
    );
}

#[test]
fn same_field_edits_produce_a_conflict() {
    let repo = new_repo();
    let root = repo.path();
    git(root, &["commit", "--allow-empty", "-q", "-m", "init"]);
    seed_task(root, "bl-bbbb");

    branch_and_commit(root, "worker-a", "bl-bbbb", "A: title X", |t| {
        let mut u = t.clone();
        u.title = "X-version".into();
        u
    });
    branch_and_commit(root, "worker-b", "bl-bbbb", "B: title Y", |t| {
        let mut u = t.clone();
        u.title = "Y-version".into();
        u
    });

    git(root, &["merge", "-q", "--no-edit", "worker-a"]);
    let clean = git_ok(root, &["merge", "--no-edit", "worker-b"]);
    assert!(
        !clean,
        "same-field edits must surface as a real conflict, not silently combine"
    );
}

#[test]
fn concurrent_note_appends_merge_cleanly() {
    let repo = new_repo();
    let root = repo.path();
    git(root, &["commit", "--allow-empty", "-q", "-m", "init"]);
    seed_task(root, "bl-cccc");

    let task_path = root.join(".balls/tasks/bl-cccc.json");

    // Worker A appends a note (touches only the sibling notes file).
    let a_path = task_path.clone();
    branch_and_raw(root, "worker-a", "A: append note", |_| {
        task_io::append_note_to(&a_path, "alice", "first").unwrap();
    });

    // Worker B appends a different note on a divergent branch.
    let b_path = task_path.clone();
    branch_and_raw(root, "worker-b", "B: append note", |_| {
        task_io::append_note_to(&b_path, "bob", "second").unwrap();
    });

    git(root, &["merge", "-q", "--no-edit", "worker-a"]);
    let merged = git_ok(root, &["merge", "--no-edit", "worker-b"]);
    assert!(
        merged,
        "concurrent note appends must merge cleanly under stock git merge"
    );

    let merged_task = Task::load(&task_path).unwrap();
    assert_eq!(merged_task.notes.len(), 2, "both notes must survive");
    let authors: Vec<&str> = merged_task.notes.iter().map(|n| n.author.as_str()).collect();
    assert!(authors.contains(&"alice"));
    assert!(authors.contains(&"bob"));
}

#[test]
fn field_edit_and_note_append_merge_cleanly() {
    let repo = new_repo();
    let root = repo.path();
    git(root, &["commit", "--allow-empty", "-q", "-m", "init"]);
    seed_task(root, "bl-dddd");

    let task_path = root.join(".balls/tasks/bl-dddd.json");

    // Worker A: field edit on the task file.
    branch_and_commit(root, "worker-a", "bl-dddd", "A: set priority", |t| {
        let mut u = t.clone();
        u.priority = 2;
        u
    });

    // Worker B: note append on the sibling file.
    let b_path = task_path.clone();
    branch_and_raw(root, "worker-b", "B: append note", |_| {
        task_io::append_note_to(&b_path, "bob", "fyi").unwrap();
    });

    git(root, &["merge", "-q", "--no-edit", "worker-a"]);
    let merged = git_ok(root, &["merge", "--no-edit", "worker-b"]);
    assert!(
        merged,
        "field edit + note append touch different files and must merge cleanly"
    );

    let final_task = Task::load(&task_path).unwrap();
    assert_eq!(final_task.priority, 2);
    assert_eq!(final_task.notes.len(), 1);
    assert_eq!(final_task.notes[0].author, "bob");
}
