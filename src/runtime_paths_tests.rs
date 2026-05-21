//! Unit tests for the runtime-path table derivations (bl-228d, bl-ebae).
//! These pin the derived sets to exactly what each consumer expects —
//! the table is only correct if every derivation stays identical to
//! the hand-kept lists it replaced.

use super::*;

#[test]
fn backstop_paths_are_the_balls_state_paths() {
    assert_eq!(
        backstop_paths(),
        vec![
            ".balls/local",
            ".balls/tasks",
            ".balls/worktree",
            ".balls/code-refs",
            ".balls/state-repo",
        ],
    );
}

#[test]
fn gitignore_paths_non_stealth_lists_every_non_federated_path() {
    assert_eq!(
        gitignore_paths(false, false),
        vec![
            ".balls/local",
            ".balls/tasks",
            ".balls/worktree",
            ".balls/code-refs",
            ".balls/state-repo",
            ".balls-worktrees",
        ],
    );
}

#[test]
fn gitignore_paths_stealth_drops_the_state_worktree() {
    // Stealth mode never creates the state worktree, so `.balls/tasks`
    // and `.balls/worktree` drop out; everything else stays ignored.
    assert_eq!(
        gitignore_paths(true, false),
        vec![
            ".balls/local",
            ".balls/code-refs",
            ".balls/state-repo",
            ".balls-worktrees",
        ],
    );
}

#[test]
fn gitignore_paths_federated_adds_plugins() {
    // `.balls/plugins` is gitignored only under `master_url`; a
    // standalone repo owns it as a real, committed directory.
    assert!(!gitignore_paths(false, false).contains(&".balls/plugins"));
    assert!(gitignore_paths(false, true).contains(&".balls/plugins"));
}

#[test]
fn federated_only_paths_are_plugins_and_canonical() {
    // bl-82a4 adds `.balls/config.json`: federated mode symlinks the
    // canonical into the hub, so it joins `.balls/plugins` as a
    // gitignored federated-only sidecar.
    assert_eq!(
        federated_only_paths(),
        vec![".balls/plugins", ".balls/config.json"]
    );
}

#[test]
fn balls_worktrees_is_gitignored_but_not_a_backstop_path() {
    // The one row that differs between consumers: `.balls-worktrees`
    // is the parent of the work worktrees, unreachable from a
    // `work/<id>` squash, so it is gitignore-only.
    assert!(gitignore_paths(false, false).contains(&".balls-worktrees"));
    assert!(!backstop_paths().contains(&".balls-worktrees"));
}
