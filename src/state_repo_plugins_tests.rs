//! bl-1098: `.balls/plugins/` symlink under master_url (option a — two
//! parallel symlinks). Covers the federated-mode materialization
//! branches in `state_repo::ensure` and the inverse `restore_plugins_dir`
//! used on detach.

use super::test_support::hub_repo;
use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn ensure_creates_plugins_symlink_in_master_url_mode() {
    // The state-repo materialization sets up `.balls/plugins -> state-repo/.balls/plugins`
    // so per-plugin config files resolve through the project root in federated mode.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();

    ensure(root.path(), &url).unwrap();

    let link = root.path().join(".balls/plugins");
    assert!(link.is_symlink(), "ensure must materialize the plugins symlink");
    let target = fs::read_link(&link).unwrap();
    assert_eq!(target, Path::new("state-repo/.balls/plugins"));
}

#[test]
fn ensure_plugins_symlink_resolves_through_project_root() {
    // The deliverable: callers reading `.balls/plugins/<name>.json` via
    // the project root must see the hub's files transparently after ensure().
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();

    ensure(root.path(), &url).unwrap();
    let hub_plugins = root.path().join(".balls/state-repo/.balls/plugins");
    fs::create_dir_all(&hub_plugins).unwrap();
    fs::write(hub_plugins.join("github.json"), "{\"token\":\"x\"}").unwrap();

    let via_project = root.path().join(".balls/plugins/github.json");
    assert_eq!(fs::read_to_string(via_project).unwrap(), "{\"token\":\"x\"}");
}

#[test]
fn ensure_plugins_symlink_idempotent() {
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    ensure(root.path(), &url).unwrap();
    ensure(root.path(), &url).unwrap();
    assert!(root.path().join(".balls/plugins").is_symlink());
}

#[test]
fn ensure_plugins_symlink_repoints_stale_target() {
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from("worktree/.balls/plugins"),
        root.path().join(".balls/plugins"),
    )
    .unwrap();

    ensure(root.path(), &url).unwrap();

    let target = fs::read_link(root.path().join(".balls/plugins")).unwrap();
    assert_eq!(target, Path::new("state-repo/.balls/plugins"));
}

#[test]
fn ensure_config_symlink_repoints_stale_target() {
    // bl-82a4: a `.balls/config.json` symlink left pointing at the
    // legacy worktree layout must be repointed at the state-repo.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from("worktree/.balls/config.json"),
        root.path().join(".balls/config.json"),
    )
    .unwrap();

    ensure(root.path(), &url).unwrap();

    let target = fs::read_link(root.path().join(".balls/config.json")).unwrap();
    assert_eq!(target, Path::new("state-repo/.balls/config.json"));
}

#[test]
fn ensure_plugins_drops_gitkeep_only_placeholder() {
    // A standalone-initted repo whose user runs `bl remaster --commit <url>`
    // has `.balls/plugins/.gitkeep` from bl init. ensure must replace
    // that placeholder dir with the symlink — the .gitkeep is just a
    // tracking anchor, not real config.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls/plugins")).unwrap();
    fs::write(root.path().join(".balls/plugins/.gitkeep"), "").unwrap();

    ensure(root.path(), &url).unwrap();

    assert!(root.path().join(".balls/plugins").is_symlink());
}

#[test]
fn ensure_plugins_refuses_dir_with_real_files() {
    // Real plugin JSON under `.balls/plugins/` would be silently shadowed
    // by the hub view after the symlink replaces the dir. Refuse with a
    // migration message so the operator decides what to move.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls/plugins")).unwrap();
    fs::write(root.path().join(".balls/plugins/github.json"), "{}").unwrap();

    let err = ensure(root.path(), &url).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("github.json"), "lists the file: {msg}");
    assert!(msg.contains("hub"), "names the migration target: {msg}");
}

#[test]
fn restore_plugins_dir_replaces_symlink_with_dir_copying_hub_files() {
    // bl-1098 detach: going standalone replaces the symlink with a real
    // dir carrying the hub's plugin files at detach time so the operator
    // keeps their plugin config across the transition.
    let root = TempDir::new().unwrap();
    let hub_plugins = root.path().join("hub_plugins");
    fs::create_dir_all(&hub_plugins).unwrap();
    fs::write(hub_plugins.join("github.json"), "{\"k\":1}").unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(&hub_plugins, root.path().join(".balls/plugins")).unwrap();

    restore_plugins_dir(root.path(), &hub_plugins).unwrap();

    let link = root.path().join(".balls/plugins");
    assert!(!link.is_symlink());
    assert!(link.is_dir());
    assert_eq!(fs::read_to_string(link.join("github.json")).unwrap(), "{\"k\":1}");
    assert!(link.join(".gitkeep").exists(), "standalone-init shape");
}

#[test]
fn restore_plugins_dir_idempotent_when_already_dir() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls/plugins")).unwrap();
    fs::write(root.path().join(".balls/plugins/marker"), "keep").unwrap();
    let src = root.path().join("absent");

    restore_plugins_dir(root.path(), &src).unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join(".balls/plugins/marker")).unwrap(),
        "keep"
    );
}

#[test]
fn restore_plugins_dir_creates_dir_when_link_absent_and_source_missing() {
    // Edge case: .balls/plugins is neither symlink nor dir (e.g., never
    // existed). Seed a fresh dir with .gitkeep so the project lands in
    // standalone-init shape.
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    let src = root.path().join("missing");

    restore_plugins_dir(root.path(), &src).unwrap();

    assert!(root.path().join(".balls/plugins").is_dir());
    assert!(root.path().join(".balls/plugins/.gitkeep").exists());
}
