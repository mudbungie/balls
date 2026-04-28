//! Shared helpers for `bl sync --review` integration tests.

#![allow(dead_code)]

use super::{bl, git};
use std::fs;
use std::path::{Path, PathBuf};

pub fn populated_sync_response(create_title: &str) -> String {
    format!(
        r#"{{
            "created": [
                {{
                    "title": "{create_title}",
                    "type": "task",
                    "priority": 2,
                    "status": "open",
                    "description": "imported from mock plugin",
                    "tags": ["from-mock"],
                    "external": {{ "remote_key": "MOCK-NEW" }}
                }}
            ],
            "updated": [],
            "deleted": []
        }}"#
    )
}

pub fn state_head(repo: &Path) -> String {
    git(repo.join(".balls/worktree").as_path(), &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

pub fn pending_dir(repo: &Path) -> PathBuf {
    repo.join(".balls/local/pending-sync/sync")
}

pub fn collect_staged_ids(repo: &Path) -> Vec<String> {
    let dir = pending_dir(repo);
    if !dir.exists() {
        return Vec::new();
    }
    fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path();
            (p.extension().and_then(|s| s.to_str()) == Some("json"))
                .then(|| {
                    p.file_stem()
                        .and_then(|s| s.to_str())
                        .map(str::to_string)
                })
                .flatten()
        })
        .collect()
}

pub fn list_tasks_count(repo: &Path) -> usize {
    let out = bl(repo)
        .args(["list", "--all", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v.as_array().map_or(0, Vec::len)
}

pub fn find_task_with_title(repo: &Path, title: &str) -> String {
    let out = bl(repo)
        .args(["list", "--all", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    for t in v.as_array().unwrap() {
        if t["title"] == title {
            return t["id"].as_str().unwrap().to_string();
        }
    }
    panic!("no task with title {title}: {v}");
}
