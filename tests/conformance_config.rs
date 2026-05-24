//! SPEC-tracker-state §16 conformance — config ownership and the
//! plugin split (tests 5 and 6).
//!
//! Each fails against the pre-split single-`config.json` model: there
//! is no `.balls/project.json`, no symlink, and `bl plugin enable`
//! writes the plugins map onto the code branch. They pass once project
//! config moves to `.balls/project.json` on the tracker branch (§7).

mod common;

use common::tracker::*;
use common::*;
use std::path::Path;

/// Overwrite one top-level field of a JSON file in place.
fn set_field(path: &Path, key: &str, val: serde_json::Value) {
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v[key] = val;
    std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

/// Test 5 — Config ownership. `.balls/project.json` is reached through
/// a symlink into the state checkout; a project-owned field set in
/// *both* files resolves to `project.json`'s value; a repo-owned
/// field is read from `config.json`.
#[test]
fn t5_config_ownership_split() {
    let repo = new_repo();
    init_in(repo.path());
    let balls = repo.path().join(".balls");

    // project.json is reached through a symlink into the state
    // checkout; config.json is a real, never-symlinked file.
    let project = balls.join("project.json");
    assert!(project.is_symlink(), ".balls/project.json must be a symlink");
    assert_eq!(
        std::fs::read_link(&project).unwrap(),
        Path::new("state-repo/.balls/project.json"),
    );
    assert!(
        !balls.join("config.json").is_symlink(),
        ".balls/config.json must be a real repo file, never symlinked",
    );

    // A project-owned field (`id_length`) set in BOTH files: the
    // project.json value wins. config.json's stale 5 is shadowed, and
    // the built-in default 4 is not used either.
    set_field(&project, "id_length", 9.into());
    set_field(&balls.join("config.json"), "id_length", 5.into());
    let id = create_task(repo.path(), "ownership");
    assert_eq!(
        id.strip_prefix("bl-").map(str::len),
        Some(9),
        "id_length must resolve to project.json's 9, got {id}",
    );

    // A repo-owned field (`worktree_dir`) is read from config.json.
    set_field(&balls.join("config.json"), "worktree_dir", "wt".into());
    bl(repo.path()).args(["claim", &id]).assert().success();
    assert!(
        repo.path().join("wt").join(&id).exists(),
        "worktree_dir from config.json must place the claim's worktree",
    );
}

/// Test 6 — Plugin split. The plugins map (`.balls/project.json`) and
/// the per-plugin config files (`.balls/plugins/`) ride the tracker
/// branch; a fresh clone inherits them; per-clone plugin auth
/// under `.balls/local/plugins/` never reaches the tracker.
#[test]
fn t6_plugin_config_rides_the_tracker_branch() {
    let tracker = new_tracker();
    let ws = new_repo();
    seed_config(ws.path(), &[("state_url", &url_of(&tracker))]);
    bl(ws.path()).arg("init").assert().success();

    bl(ws.path())
        .args(["plugin", "enable", "github-issues"])
        .assert()
        .success();

    // Per-clone plugin auth — must never leave the clone.
    let auth = ws.path().join(".balls/local/plugins/github-issues");
    std::fs::create_dir_all(&auth).unwrap();
    std::fs::write(auth.join("token.json"), r#"{"token":"secret"}"#).unwrap();

    bl(ws.path()).arg("sync").assert().success();

    // The project config and the plugin config file are on the tracker
    // branch; the per-clone auth is not.
    let tree = git(tracker.path(), &["ls-tree", "-r", "--name-only", "balls/tasks"]);
    assert!(
        tree.contains(".balls/project.json"),
        "project.json must ride the tracker branch:\n{tree}",
    );
    assert!(
        tree.contains(".balls/plugins/github-issues.json"),
        "the plugin config file must ride the tracker branch:\n{tree}",
    );
    assert!(
        !tree.contains(".balls/local"),
        "per-clone plugin auth must never reach the tracker:\n{tree}",
    );

    // A fresh clone of the same tracker inherits the enabled plugin.
    let ws2 = new_repo();
    seed_config(ws2.path(), &[("state_url", &url_of(&tracker))]);
    bl(ws2.path()).arg("init").assert().success();
    let out = bl(ws2.path()).args(["plugin", "list"]).output().unwrap();
    assert!(out.status.success());
    let listed = String::from_utf8_lossy(&out.stdout);
    assert!(
        listed.contains("github-issues"),
        "a fresh clone must inherit the project's plugin map:\n{listed}",
    );
}
