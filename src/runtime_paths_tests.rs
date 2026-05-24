//! Unit tests for the runtime-path table derivations. These pin the
//! derived sets to exactly what each consumer expects — the table is
//! only correct if every derivation stays identical to the hand-kept
//! lists it replaced.

use super::*;

#[test]
fn backstop_paths_are_the_state_checkout_paths() {
    assert_eq!(
        backstop_paths(),
        vec![
            ".balls/local",
            ".balls/tasks",
            ".balls/project.json",
            ".balls/state-repo",
            ".balls/plugins",
            ".balls/code-refs",
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
            ".balls/project.json",
            ".balls/state-repo",
            ".balls/plugins",
            ".balls/code-refs",
            ".balls-worktrees",
        ],
    );
}

#[test]
fn gitignore_paths_stealth_drops_the_state_checkout() {
    // Stealth mode never creates the state checkout, so `.balls/tasks`,
    // `.balls/project.json`, `.balls/state-repo`, and `.balls/plugins`
    // drop out.
    assert_eq!(
        gitignore_paths(true),
        vec![".balls/local", ".balls/code-refs", ".balls-worktrees"],
    );
}

#[test]
fn config_json_is_never_a_runtime_path() {
    // `.balls/config.json` is a committed, repo-owned deliverable.
    assert!(!gitignore_paths(false).contains(&".balls/config.json"));
    assert!(!backstop_paths().contains(&".balls/config.json"));
}

#[test]
fn balls_worktrees_is_gitignored_but_not_a_backstop_path() {
    // `.balls-worktrees` is the parent of the work worktrees,
    // unreachable from a `work/<id>` squash, so it is gitignore-only.
    assert!(gitignore_paths(false).contains(&".balls-worktrees"));
    assert!(!backstop_paths().contains(&".balls-worktrees"));
}

#[test]
fn plugins_is_gitignored_and_a_backstop_path() {
    // `.balls/plugins` is a symlink on a fully migrated clone, but
    // a pre-bl-de57 legacy repo can still carry the committed
    // `.balls/plugins/*.json` index entries on a work branch — the
    // backstop keeps those out of a review squash.
    assert!(gitignore_paths(false).contains(&".balls/plugins"));
    assert!(backstop_paths().contains(&".balls/plugins"));
}
