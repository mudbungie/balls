//! Unit tests for the §11 pure path/string derivations — worktree path /
//! branch / subject / marker arithmetic and the invocation-path guard. No git,
//! no IO: balls prints the same paths from the same formulas.

use super::*;

#[test]
fn worktree_path_mirrors_the_invocation_path_for_a_cargo_safe_dir() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    let p = worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-f813");
    // The code worktree MIRRORS the invocation path (no percent-encoding): a `%`
    // ancestor breaks `rust-lld`'s output paths (bl-f3e4). The leading `/` is
    // stripped so it nests under the territory; the result has no `%`.
    assert_eq!(
        p,
        Path::new("/st/balls/plugins/delivery/home/me/dev/proj/bl-f813")
    );
    assert!(!p.to_string_lossy().contains('%'));
}

#[test]
fn work_branch_is_the_branch_half_of_the_worktree_pair() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    // The branch and path derive from the same `<id>` key through the one pair
    // of helpers — the convergence §11 claimant-keying will edit in one place.
    assert_eq!(work_branch("bl-f813"), "work/bl-f813");
    let p = worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-f813");
    assert_eq!(work_branch("bl-f813"), format!("work/{}", p.file_name().unwrap().to_str().unwrap()));
}

#[test]
fn subject_and_marker_carry_the_delivery_tag() {
    assert_eq!(subject("Refactor foo", "bl-f813"), "Refactor foo [bl-f813]");
    assert_eq!(marker("bl-f813"), "[bl-f813]");
}

#[test]
fn binding_territory_is_the_parent_of_every_worktree() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    let territory = binding_territory(&xdg, "delivery", "/home/me/dev/proj");
    assert_eq!(territory, worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-x").parent().unwrap());
}

#[test]
fn ensure_safe_invocation_path_admits_clean_absolute_paths() {
    assert!(ensure_safe_invocation_path("/home/mark/dev/balls").is_ok());
    // A literal `..`-prefixed filename (no separator) is fine — it cannot escape.
    assert!(ensure_safe_invocation_path("/home/mark/..foo").is_ok());
}

#[test]
fn ensure_safe_invocation_path_rejects_relative_and_dotdot() {
    assert!(ensure_safe_invocation_path("home/mark/dev").is_err()); // not absolute
    assert!(ensure_safe_invocation_path("/home/../../etc").is_err()); // `..` traversal
}
