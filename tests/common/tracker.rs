//! Helpers for the unified tracker-state model
//! (`docs/SPEC-tracker-state.md`): standing up a bare *tracker* — a
//! git repo hosting the shared `balls/tasks` branch — and reading the
//! tracker address out of a workspace's committed `config.json`.

#![allow(dead_code)]

use super::{git, new_bare_remote, tmp, Repo};
use std::path::Path;

/// A bare git repo carrying a seeded `balls/tasks` orphan branch — the
/// minimal "tracker" a `state_url` points at. Built with stock git so
/// it never depends on `bl`'s own checkout behaviour.
pub fn new_tracker() -> Repo {
    let bare = new_bare_remote();
    let seed = tmp();
    let s = seed.path();
    git(s, &["init", "-q", "-b", "balls/tasks"]);
    git(s, &["config", "user.email", "tracker@example.com"]);
    git(s, &["config", "user.name", "tracker"]);
    git(s, &["config", "commit.gpgsign", "false"]);
    let tasks = s.join(".balls/tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    std::fs::write(tasks.join(".gitattributes"), "*.notes.jsonl merge=union\n").unwrap();
    std::fs::write(tasks.join(".gitkeep"), "").unwrap();
    std::fs::create_dir_all(s.join(".balls/plugins")).unwrap();
    std::fs::write(s.join(".balls/plugins/.gitkeep"), "").unwrap();
    git(s, &["add", "-A"]);
    git(s, &["commit", "-qm", "balls: seed state branch", "--no-verify"]);
    git(s, &["push", "-q", &url_of(&bare), "balls/tasks"]);
    bare
}

/// URL string for a repo path (a tracker or a code remote).
pub fn url_of(repo: &Repo) -> String {
    repo.path().to_string_lossy().to_string()
}

/// Read `state_url` from a workspace's committed `.balls/config.json`.
pub fn state_url(repo: &Path) -> Option<String> {
    config_field(repo, "state_url")
}

/// Read a string-valued field from a workspace's `.balls/config.json`.
pub fn config_field(repo: &Path, key: &str) -> Option<String> {
    let s = std::fs::read_to_string(repo.join(".balls/config.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get(key)?.as_str().map(String::from)
}

/// Task ids present on `balls/tasks` in a bare tracker.
pub fn tracker_task_ids(tracker: &Path) -> Vec<String> {
    let listing = git(tracker, &["ls-tree", "-r", "--name-only", "balls/tasks"]);
    listing
        .lines()
        .filter_map(|l| l.rsplit('/').next())
        .filter_map(|n| n.strip_suffix(".json"))
        .map(String::from)
        .collect()
}
