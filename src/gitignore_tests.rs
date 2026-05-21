//! Unit coverage for the `.gitignore` file mechanic. Which paths land
//! in the file is `runtime_paths`' contract — tested there; here we
//! pin the append behaviour and the stealth arm.

use super::*;
use tempfile::TempDir;

fn read_gitignore(d: &TempDir) -> String {
    fs::read_to_string(d.path().join(".gitignore")).unwrap()
}

fn has_line(d: &TempDir, entry: &str) -> bool {
    read_gitignore(d).lines().any(|l| l == entry)
}

#[test]
fn ensure_creates_gitignore_with_runtime_paths() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), false).unwrap();
    for entry in [".balls/local", ".balls-worktrees", ".balls/state-repo", ".balls/plugins", ".balls/tasks"] {
        assert!(has_line(&d, entry), "missing {entry}");
    }
    assert!(
        !has_line(&d, ".balls/config.json"),
        "config.json is a committed deliverable, never gitignored"
    );
}

#[test]
fn ensure_stealth_omits_state_checkout_paths() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), true).unwrap();
    assert!(has_line(&d, ".balls/local"));
    assert!(!has_line(&d, ".balls/tasks"));
    assert!(!has_line(&d, ".balls/state-repo"));
}

#[test]
fn ensure_is_idempotent() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), false).unwrap();
    let first = read_gitignore(&d);
    ensure_main_gitignore(d.path(), false).unwrap();
    assert_eq!(first, read_gitignore(&d));
}

#[test]
fn ensure_appends_newline_to_unterminated_file() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "target").unwrap();
    ensure_main_gitignore(d.path(), false).unwrap();
    let c = read_gitignore(&d);
    assert!(c.starts_with("target\n"), "{c}");
    assert!(has_line(&d, ".balls/local"));
}

#[test]
fn ensure_skips_paths_already_present() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), ".balls/local\ntarget\n").unwrap();
    ensure_main_gitignore(d.path(), false).unwrap();
    let c = read_gitignore(&d);
    assert_eq!(c.matches(".balls/local\n").count(), 1, "no duplicate line: {c}");
    assert!(has_line(&d, ".balls/state-repo"));
}
