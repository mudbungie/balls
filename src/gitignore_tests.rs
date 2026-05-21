//! Unit coverage for the `.gitignore` file mechanic (bl-ebae). Which
//! paths land in the file is `runtime_paths`' contract — tested there;
//! here we pin append/remove behaviour and the federated arm's effect.

use super::*;
use tempfile::TempDir;

fn read_gitignore(d: &TempDir) -> String {
    fs::read_to_string(d.path().join(".gitignore")).unwrap()
}

fn has_line(d: &TempDir, entry: &str) -> bool {
    read_gitignore(d).lines().any(|l| l == entry)
}

#[test]
fn ensure_creates_gitignore_with_non_federated_paths() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), false, false).unwrap();
    for entry in [".balls/local", ".balls-worktrees", ".balls/state-repo"] {
        assert!(has_line(&d, entry), "missing {entry}");
    }
    assert!(!has_line(&d, ".balls/plugins"), "non-federated keeps plugins out");
}

#[test]
fn ensure_stealth_omits_state_worktree_paths() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), true, false).unwrap();
    assert!(has_line(&d, ".balls/local"));
    assert!(!has_line(&d, ".balls/tasks"));
    assert!(!has_line(&d, ".balls/worktree"));
}

#[test]
fn ensure_federated_adds_plugins() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), false, true).unwrap();
    assert!(has_line(&d, ".balls/plugins"));
    assert!(has_line(&d, ".balls/state-repo"));
}

#[test]
fn ensure_is_idempotent() {
    let d = TempDir::new().unwrap();
    ensure_main_gitignore(d.path(), false, true).unwrap();
    let first = read_gitignore(&d);
    ensure_main_gitignore(d.path(), false, true).unwrap();
    assert_eq!(first, read_gitignore(&d));
}

#[test]
fn ensure_appends_newline_to_unterminated_file() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "target").unwrap();
    ensure_main_gitignore(d.path(), false, false).unwrap();
    let c = read_gitignore(&d);
    assert!(c.starts_with("target\n"), "{c}");
    assert!(has_line(&d, ".balls/local"));
}

#[test]
fn remove_entries_drops_only_named_lines() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join(".gitignore"),
        "target\n.balls/plugins\n.balls/local\n",
    )
    .unwrap();
    remove_entries(d.path(), &[".balls/plugins"]).unwrap();
    assert!(!has_line(&d, ".balls/plugins"));
    assert!(has_line(&d, "target"));
    assert!(has_line(&d, ".balls/local"));
}

#[test]
fn remove_entries_missing_file_is_noop() {
    let d = TempDir::new().unwrap();
    remove_entries(d.path(), &[".balls/plugins"]).unwrap();
    assert!(!d.path().join(".gitignore").exists());
}

#[test]
fn remove_entries_absent_line_leaves_file_untouched() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "target\n").unwrap();
    remove_entries(d.path(), &[".balls/plugins"]).unwrap();
    assert_eq!(read_gitignore(&d), "target\n");
}

#[test]
fn remove_entries_can_empty_the_file() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), ".balls/plugins\n").unwrap();
    remove_entries(d.path(), &[".balls/plugins"]).unwrap();
    assert_eq!(read_gitignore(&d), "");
}
