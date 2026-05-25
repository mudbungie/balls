//! Unit tests for `migrate::steps` pure helpers. The step functions
//! that shell out to git are covered by the integration tests in
//! `tests/conformance_migrate*.rs`.

use super::*;
use crate::config::{Delivery, DeliveryMode, ReviewConfig};

fn cfg() -> Config {
    Config::default()
}

#[test]
fn translate_local_squash_to_direct() {
    let mut c = cfg();
    c.delivery = Some(Delivery { mode: DeliveryMode::LocalSquash });
    let r = translate_repo_config(&c);
    let i = r.integrate.expect("integrate set");
    assert_eq!(i.mode, IntegrateMode::Direct);
}

#[test]
fn translate_deferred_to_forge_pr() {
    let mut c = cfg();
    c.delivery = Some(Delivery { mode: DeliveryMode::Deferred });
    let r = translate_repo_config(&c);
    assert_eq!(r.integrate.unwrap().mode, IntegrateMode::ForgePr);
}

#[test]
fn translate_pre_check_to_gate_command() {
    let mut c = cfg();
    c.review = Some(ReviewConfig { pre_check: Some("make check".into()) });
    let r = translate_repo_config(&c);
    let rb = r.review.expect("review set");
    assert_eq!(rb.gate_command.as_deref(), Some("make check"));
}

#[test]
fn translate_absent_review_yields_no_review_block() {
    let r = translate_repo_config(&cfg());
    assert!(r.review.is_none());
    assert!(r.integrate.is_none());
}

#[test]
fn translate_review_with_no_pre_check_yields_no_review_block() {
    let mut c = cfg();
    c.review = Some(ReviewConfig { pre_check: None });
    let r = translate_repo_config(&c);
    assert!(r.review.is_none());
}

#[test]
fn translate_carries_repo_only_fields_verbatim() {
    let mut c = cfg();
    c.protected_main = true;
    c.stale_threshold_seconds = 12345;
    c.auto_fetch_on_ready = false;
    let r = translate_repo_config(&c);
    assert!(r.protected_main);
    assert_eq!(r.stale_threshold_seconds, 12345);
    assert!(!r.auto_fetch_on_ready);
}

#[test]
fn is_balls_gitignore_line_matches_known_patterns() {
    assert!(is_balls_gitignore_line(".balls"));
    assert!(is_balls_gitignore_line(".balls/state-repo"));
    assert!(is_balls_gitignore_line(".balls-worktrees"));
    assert!(is_balls_gitignore_line(".balls-worktrees/foo"));
}

#[test]
fn is_balls_gitignore_line_ignores_unrelated_paths() {
    assert!(!is_balls_gitignore_line(""));
    assert!(!is_balls_gitignore_line("target/"));
    assert!(!is_balls_gitignore_line(".balls-leftover"));
}

#[test]
fn copy_tree_contents_is_a_noop_for_missing_src() {
    let dst = tempfile::TempDir::new().unwrap();
    copy_tree_contents(Path::new("/no/such/path"), dst.path()).expect("noop");
}

#[test]
fn copy_tree_contents_recurses_into_subdirs_and_skips_existing() {
    let src = tempfile::TempDir::new().unwrap();
    let dst = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(src.path().join("sub")).unwrap();
    fs::write(src.path().join("file.txt"), "hello").unwrap();
    fs::write(src.path().join("sub/nested.txt"), "world").unwrap();
    // Pre-existing entry at dst keeps its contents (idempotence).
    fs::write(dst.path().join("file.txt"), "kept").unwrap();
    copy_tree_contents(src.path(), dst.path()).expect("copy");
    assert_eq!(fs::read_to_string(dst.path().join("file.txt")).unwrap(), "kept");
    assert_eq!(
        fs::read_to_string(dst.path().join("sub/nested.txt")).unwrap(),
        "world"
    );
}

#[test]
fn strip_balls_gitignore_is_noop_when_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    strip_balls_gitignore(dir.path()).expect("noop");
}

#[test]
fn strip_balls_gitignore_preserves_non_balls_lines() {
    // strip_balls_gitignore writes back via `fs::write` (the "rewritten
    // != original" branch), exercising the non-trivial path. We skip
    // the `git add` follow-up by setting up a non-git dir — the call
    // bails on the git rm if the file ends up empty.
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(
        dir.path().join(".gitignore"),
        "target/\n.balls/state-repo\n.balls-worktrees\nnode_modules/\n",
    )
    .unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["init", "-q", "-b", "main"])
        .status()
        .unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["add", ".gitignore"])
        .status()
        .unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["-c", "user.email=t@x", "-c", "user.name=t", "commit", "-m", "x", "--no-verify"])
        .status()
        .unwrap();
    strip_balls_gitignore(dir.path()).expect("strip");
    let out = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(out.contains("target/"));
    assert!(out.contains("node_modules/"));
    assert!(!out.contains(".balls"));
}
