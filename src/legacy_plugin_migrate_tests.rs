//! Unit coverage for the legacy-plugin migration commit. Fixtures are
//! real git repos in tempdirs — the migration is a code-branch index
//! mutation, so a stubbed git would not exercise the path.

use super::*;
use crate::git_test_support::{git_run, git_stdout, init_repo, init_repo_no_commit};
use tempfile::TempDir;

/// A workspace shaped like a pre-bl-8a9a repo: an initial commit on
/// `main` with `.balls/plugins/{github.json,.gitkeep}` tracked and a
/// stale `.gitignore` that lacks the unified runtime paths.
fn legacy_workspace() -> TempDir {
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo_no_commit(p);
    std::fs::create_dir_all(p.join(".balls/plugins")).unwrap();
    std::fs::write(p.join(".balls/plugins/github.json"), "{}\n").unwrap();
    std::fs::write(p.join(".balls/plugins/.gitkeep"), "").unwrap();
    // Pre-bl-8a9a `.gitignore`: has `.balls/worktree` but not
    // `.balls/state-repo` or `.balls/plugins` — the live dogfood shape.
    std::fs::write(p.join(".gitignore"), ".balls/local\n.balls/worktree\n").unwrap();
    std::fs::write(p.join("README.md"), "x\n").unwrap();
    git_run(p, &["add", "-A"]);
    git_run(p, &["commit", "-qm", "pre-bl-8a9a layout", "--no-verify"]);
    d
}

fn head_sha(p: &std::path::Path) -> String {
    git_stdout(p, &["rev-parse", "HEAD"])
}

#[test]
fn run_rms_legacy_plugin_files_and_refreshes_gitignore() {
    let d = legacy_workspace();
    let p = d.path();
    let before = head_sha(p);

    run(p).unwrap();

    let after = head_sha(p);
    assert_ne!(before, after, "a migration commit landed on HEAD");
    let subject = git_stdout(p, &["log", "-1", "--format=%s"]);
    assert_eq!(subject, COMMIT_MSG);

    let tree = git_stdout(p, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(
        !tree.contains(".balls/plugins/github.json"),
        "legacy plugin config unstaged: {tree}"
    );
    assert!(
        !tree.contains(".balls/plugins/.gitkeep"),
        "legacy gitkeep unstaged: {tree}"
    );

    let gi = std::fs::read_to_string(p.join(".gitignore")).unwrap();
    assert!(gi.contains(".balls/state-repo"), "embedded-repo hazard closed: {gi}");
    assert!(gi.contains(".balls/plugins"), "symlink gitignored: {gi}");
}

#[test]
fn run_is_a_no_op_on_a_clean_post_migration_workspace() {
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo_no_commit(p);
    let lines = crate::runtime_paths::gitignore_paths(false).join("\n") + "\n";
    std::fs::write(p.join(".gitignore"), lines).unwrap();
    std::fs::write(p.join("README.md"), "x\n").unwrap();
    git_run(p, &["add", "-A"]);
    git_run(p, &["commit", "-qm", "post-migration", "--no-verify"]);
    let before = head_sha(p);

    run(p).unwrap();

    assert_eq!(before, head_sha(p), "no commit when nothing to migrate");
}

#[test]
fn run_handles_a_legacy_index_with_a_fresh_gitignore() {
    // Belt-and-braces: the index still has the legacy plugin files but
    // someone already fixed the `.gitignore` by hand. The rm-and-commit
    // step must still run.
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo_no_commit(p);
    std::fs::create_dir_all(p.join(".balls/plugins")).unwrap();
    std::fs::write(p.join(".balls/plugins/github.json"), "{}\n").unwrap();
    let lines = crate::runtime_paths::gitignore_paths(false).join("\n") + "\n";
    std::fs::write(p.join(".gitignore"), lines).unwrap();
    git_run(p, &["add", "-A"]);
    git_run(p, &["commit", "-qm", "mixed legacy", "--no-verify"]);

    run(p).unwrap();

    let tree = git_stdout(p, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!tree.contains(".balls/plugins/github.json"), "{tree}");
}

#[test]
fn run_writes_a_gitignore_when_one_does_not_exist() {
    // The legacy workspace shape that has no `.gitignore` at all: the
    // file is created with the runtime-path entries and committed.
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo(p);
    assert!(!p.join(".gitignore").exists());

    run(p).unwrap();

    assert!(p.join(".gitignore").exists(), "gitignore created");
    let subject = git_stdout(p, &["log", "-1", "--format=%s"]);
    assert_eq!(subject, COMMIT_MSG, "the gitignore-only case still commits");
}

#[test]
fn run_skips_a_bare_repo() {
    // A bare hub has no working tree; the migration commit cannot land
    // here, it lands on the bare via a downstream clone's push.
    let d = TempDir::new().unwrap();
    let p = d.path();
    git_run(p, &["init", "-q", "--bare"]);
    run(p).unwrap();
    // No commits added — bare repos have no HEAD until something is pushed.
}

#[test]
fn run_skips_a_non_git_directory() {
    let d = TempDir::new().unwrap();
    run(d.path()).unwrap();
}

#[test]
fn run_skips_a_repo_with_no_commits() {
    // A freshly `git init`-ed workspace has no HEAD; the migration must
    // not invent an initial commit.
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo_no_commit(p);
    run(p).unwrap();
    assert!(!crate::git::git_has_any_commits(p), "no fabricated initial commit");
}

#[test]
fn legacy_paths_in_head_returns_empty_on_a_branch_without_balls_plugins() {
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo(p);
    let paths = legacy_paths_in_head(p).unwrap();
    assert!(paths.is_empty());
}

#[test]
fn legacy_paths_in_head_returns_empty_on_an_unborn_head() {
    // `git ls-tree HEAD ...` exits non-zero before the first commit;
    // the helper short-circuits to empty rather than propagating —
    // `run` only reaches it when HEAD exists, but the guard is the
    // single-source defence against a misuse from elsewhere.
    let d = TempDir::new().unwrap();
    let p = d.path();
    init_repo_no_commit(p);
    let paths = legacy_paths_in_head(p).unwrap();
    assert!(paths.is_empty());
}

#[test]
fn state_repo_ensure_drives_the_migration_on_a_legacy_workspace() {
    // bl-de57 wiring check: `state_repo::ensure`, the cold-path
    // entry, calls `run` so a legacy workspace migrates without an
    // explicit user step. Tracker shape is the minimum a bare URL
    // needs to materialize the state checkout.
    use crate::tracker_address::Address;
    let tracker = TempDir::new().unwrap();
    let tdir = tracker.path().join("t.git");
    git_run(tracker.path(), &["init", "-q", "--bare", tdir.to_str().unwrap()]);
    let d = legacy_workspace();
    let addr = Address {
        url: Some(tdir.to_string_lossy().into_owned()),
        branch: "balls/tasks".into(),
        explicit: true,
    };
    crate::state_repo::ensure(d.path(), &addr).unwrap();
    let subject = git_stdout(d.path(), &["log", "-1", "--format=%s"]);
    assert_eq!(subject, COMMIT_MSG);
}
