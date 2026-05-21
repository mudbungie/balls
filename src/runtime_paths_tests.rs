//! Unit tests for the runtime-path table derivations (bl-228d).
//! These pin the two derived sets to exactly what the consumers
//! produced before the table existed — the refactor is only correct
//! if both sets are identical to the old hand-kept lists.

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
fn gitignore_paths_non_stealth_lists_every_runtime_path() {
    assert_eq!(
        gitignore_paths(false),
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
        gitignore_paths(true),
        vec![
            ".balls/local",
            ".balls/code-refs",
            ".balls/state-repo",
            ".balls-worktrees",
        ],
    );
}

#[test]
fn balls_worktrees_is_gitignored_but_not_a_backstop_path() {
    // The one row that differs between consumers: `.balls-worktrees`
    // is the parent of the work worktrees, unreachable from a
    // `work/<id>` squash, so it is gitignore-only.
    assert!(gitignore_paths(false).contains(&".balls-worktrees"));
    assert!(!backstop_paths().contains(&".balls-worktrees"));
}
