//! bl-ebae — `bl remaster`'s federated flip and `--detach` leave a
//! clean `git status`.
//!
//! When a standalone repo flips to `master_url`, `state_repo::ensure`
//! swaps `.balls/plugins/` for a symlink into the balls-owned hub
//! clone. Without the git-hygiene side of that transition the repo is
//! left with a tracked-but-deleted `.balls/plugins/.gitkeep` and two
//! untracked, un-ignored sidecars (`.balls/plugins`, `.balls/state-repo`)
//! that a careless `git add -A` would bake into the shared repo.
//!
//! Conformance:
//! - `bl remaster --commit <url>` gitignores the federated sidecars,
//!   untracks the `.gitkeep`, and commits the flip — clean tree after.
//! - The flip is idempotent: a re-run adds no second commit.
//! - `bl remaster --detach` reverses it: `.balls/plugins` leaves the
//!   ignore list, its `.gitkeep` is tracked again, tree stays clean.
//! - A `bl init` against a seeded `master_url` config is clean too.

mod common;

use common::*;
use std::fs;
use std::path::Path;

const FLIP_SUBJECT: &str = "balls: remaster to federated hub";

/// `git status --porcelain` with nothing left to commit.
fn status_is_clean(repo: &Path) -> bool {
    git(repo, &["status", "--porcelain"]).trim().is_empty()
}

/// Lines of `.gitignore`, or empty when the file is absent.
fn gitignore_lines(repo: &Path) -> Vec<String> {
    fs::read_to_string(repo.join(".gitignore"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether `path` is tracked in the index.
fn is_tracked(repo: &Path, path: &str) -> bool {
    !git(repo, &["ls-files", "--", path]).trim().is_empty()
}

fn commit_subjects(repo: &Path) -> Vec<String> {
    git(repo, &["log", "--format=%s"])
        .lines()
        .map(str::to_string)
        .collect()
}

/// Seed a non-stealth config carrying `master_url` so `bl init` takes
/// the federated leg directly.
fn seed_master_url_config(repo: &Path, hub_url: &str) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,
                 "worktree_dir":".balls-worktrees","master_url":"{hub_url}"}}"#
        ),
    )
    .unwrap();
}

/// The federated flip commits the master_url transition and leaves no
/// stray working-tree changes behind.
#[test]
fn remaster_commit_leaves_clean_git_status() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    assert!(
        status_is_clean(alice.path()),
        "federated flip must leave a clean git status: {}",
        git(alice.path(), &["status", "--porcelain"])
    );
    assert!(
        commit_subjects(alice.path()).iter().any(|s| s == FLIP_SUBJECT),
        "flip must land its own commit"
    );
}

/// The flip ignores both balls-owned sidecars and drops the
/// now-symlink-shadowed plugins `.gitkeep` from the index.
#[test]
fn remaster_commit_gitignores_federated_paths_and_untracks_gitkeep() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    assert!(
        is_tracked(alice.path(), ".balls/plugins/.gitkeep"),
        "standalone init tracks the plugins .gitkeep"
    );

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    let gi = gitignore_lines(alice.path());
    assert!(gi.iter().any(|l| l == ".balls/plugins"), "{gi:?}");
    assert!(gi.iter().any(|l| l == ".balls/state-repo"), "{gi:?}");
    assert!(
        !is_tracked(alice.path(), ".balls/plugins/.gitkeep"),
        "the federated flip must untrack the plugins .gitkeep"
    );
}

/// Re-running the flip against the same hub is a no-op — no second
/// commit, tree still clean.
#[test]
fn remaster_commit_is_idempotent() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    let hub_url = hub.path().to_string_lossy().to_string();

    for _ in 0..2 {
        bl(alice.path())
            .arg("remaster")
            .arg(&hub_url)
            .arg("--commit")
            .assert()
            .success();
    }

    assert!(status_is_clean(alice.path()), "re-run must stay clean");
    let flips = commit_subjects(alice.path())
        .iter()
        .filter(|s| *s == FLIP_SUBJECT)
        .count();
    assert_eq!(flips, 1, "the flip must commit exactly once");
}

/// `bl remaster --detach` reverses the flip: `.balls/plugins` leaves
/// the ignore list and is a tracked directory again, `.balls/state-repo`
/// stays ignored (the clone is still on disk), tree stays clean.
#[test]
fn remaster_detach_restores_standalone_shape() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    let hub_url = hub.path().to_string_lossy().to_string();

    bl(alice.path())
        .arg("remaster")
        .arg(&hub_url)
        .arg("--commit")
        .assert()
        .success();
    bl(alice.path())
        .arg("remaster")
        .arg("--detach")
        .assert()
        .success();

    let gi = gitignore_lines(alice.path());
    assert!(
        !gi.iter().any(|l| l == ".balls/plugins"),
        "detach must drop .balls/plugins from .gitignore: {gi:?}"
    );
    assert!(
        gi.iter().any(|l| l == ".balls/state-repo"),
        "the state-repo clone is still on disk — it stays ignored"
    );
    assert!(
        is_tracked(alice.path(), ".balls/plugins/.gitkeep"),
        "detach must re-track the plugins .gitkeep"
    );
    assert!(
        status_is_clean(alice.path()),
        "detach must leave a clean git status: {}",
        git(alice.path(), &["status", "--porcelain"])
    );
}

/// `bl init` against a config that already carries `master_url` takes
/// the federated leg and is clean — no manual gitignore surgery.
#[test]
fn bl_init_with_seeded_master_url_is_clean() {
    let hub = new_bare_remote();
    let alice = new_repo();
    seed_master_url_config(alice.path(), hub.path().to_string_lossy().as_ref());

    bl(alice.path()).arg("init").assert().success();

    let gi = gitignore_lines(alice.path());
    assert!(gi.iter().any(|l| l == ".balls/plugins"), "{gi:?}");
    assert!(gi.iter().any(|l| l == ".balls/state-repo"), "{gi:?}");
    assert!(
        status_is_clean(alice.path()),
        "seeded-master_url init must be clean: {}",
        git(alice.path(), &["status", "--porcelain"])
    );
}
