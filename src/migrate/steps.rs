//! Per-step implementations for `bl migrate` (SPEC-clone-layout
//! §11.1). The `migrate` module orchestrates these in order; this
//! file is the bag of "what each step actually does." Line-cap split
//! per the project decomposition convention.
//!
//! Step order (load-bearing per §11.1):
//!
//! 1. [`materialize_tracker_checkout`] — clone the state branch into
//!    the nested XDG path.
//! 2. [`write_xdg_config_files`] — translate `.balls/config.json` to
//!    `repo.json` (+ optional `tracker.json`), commit on the state
//!    branch, best-effort push to `origin`.
//! 3. [`move_per_clone_state`] — copy claims/locks/plugin-auth into
//!    the XDG per-clone tree.
//! 4. [`move_worktrees`] — `git worktree move` each task worktree to
//!    its XDG path.
//! 5. [`cleanup_on_main`] — last; `git rm` `.balls/`, strip
//!    `.gitignore` entries, commit the result.

use super::{MigrationPlan, STATE_COMMIT};
use crate::config::{Config, DeliveryMode};
use crate::error::{BallError, Result};
use crate::git;
use crate::layered_fields::{Integrate, IntegrateMode, ReviewBlock};
use crate::repo_json::RepoJson;
use crate::tracker_json::TrackerJson;
use std::fs;
use std::path::Path;

pub(super) fn materialize_tracker_checkout(
    root: &Path,
    plan: &MigrationPlan,
    origin_url: &str,
) -> Result<()> {
    // A previously-completed `materialize` step leaves the XDG checkout
    // in place; re-runs skip the clone and only re-ensure the user so a
    // partially-applied migration's commit step still succeeds.
    if !plan.tracker_checkout.join(".git").exists() {
        if let Some(parent) = plan.tracker_checkout.parent() {
            fs::create_dir_all(parent)?;
        }
        // Clone from the local state-repo (offline-friendly) then
        // re-target `origin` to the real URL so future fetches reach
        // the upstream tracker. bl-init writes the state-repo as part
        // of every legacy clone.
        let source = root.join(".balls/state-repo");
        git::run_git_ok(
            Path::new("."),
            &[
                "clone",
                "-q",
                "--single-branch",
                "--branch",
                "balls/tasks",
                &source.to_string_lossy(),
                &plan.tracker_checkout.to_string_lossy(),
            ],
        )?;
        git::git_config_set(&plan.tracker_checkout, "remote.origin.url", origin_url)?;
    }
    git::git_ensure_user(&plan.tracker_checkout)?;
    Ok(())
}

pub(super) fn write_xdg_config_files(
    plan: &MigrationPlan,
    legacy: &Config,
    notes: &mut Vec<String>,
) -> Result<()> {
    let dot_balls = plan.tracker_checkout.join(".balls");
    fs::create_dir_all(&dot_balls)?;
    let repo = translate_repo_config(legacy);
    fs::write(dot_balls.join("repo.json"), serde_json::to_string_pretty(&repo)? + "\n")?;
    if legacy.target_branch.is_some() {
        notes.push(
            "warning: repo-level `target_branch` dropped on migration; use \
             `task.target_branch` per task or check out the intended branch \
             at the repo root (SPEC-clone-layout §6.7)"
                .into(),
        );
    }
    let mut staged: Vec<&Path> = vec![Path::new(".balls/repo.json")];
    if let Some(url) = legacy.state_url.as_ref() {
        let tj = TrackerJson {
            state_url: url.clone(),
            state_branch: legacy.state_branch.clone(),
        };
        fs::write(dot_balls.join("tracker.json"), tj.to_json()? + "\n")?;
        staged.push(Path::new(".balls/tracker.json"));
    }
    git::git_add(&plan.tracker_checkout, &staged)?;
    if git::has_staged_changes(&plan.tracker_checkout)? {
        git::git_commit(&plan.tracker_checkout, STATE_COMMIT)?;
        let _ = git::git_push(&plan.tracker_checkout, "origin", "balls/tasks");
    }
    Ok(())
}

/// Translate a legacy `Config` into the new `RepoJson` shape per
/// SPEC §6.6 (field renames) + §6.7 (target_branch removed).
fn translate_repo_config(legacy: &Config) -> RepoJson {
    let integrate = legacy.delivery.as_ref().map(|d| Integrate {
        mode: match d.mode {
            DeliveryMode::LocalSquash => IntegrateMode::Direct,
            DeliveryMode::Deferred => IntegrateMode::ForgePr,
        },
    });
    let review = legacy.review.as_ref().and_then(|r| {
        r.pre_check
            .as_ref()
            .map(|cmd| ReviewBlock { gate_command: Some(cmd.clone()) })
    });
    RepoJson {
        integrate,
        review,
        require_remote_on_claim: Some(legacy.require_remote_on_claim),
        require_remote_on_review: Some(legacy.require_remote_on_review),
        require_remote_on_close: Some(legacy.require_remote_on_close),
        auto_fetch_on_ready: legacy.auto_fetch_on_ready,
        stale_threshold_seconds: legacy.stale_threshold_seconds,
        worktree_dir: None,
        protected_main: legacy.protected_main,
    }
}

pub(super) fn move_per_clone_state(root: &Path, plan: &MigrationPlan) -> Result<()> {
    let local = root.join(".balls/local");
    fs::create_dir_all(&plan.per_clone.claims)?;
    fs::create_dir_all(&plan.per_clone.locks)?;
    fs::create_dir_all(&plan.per_clone.plugins_auth)?;
    copy_tree_contents(&local.join("claims"), &plan.per_clone.claims)?;
    copy_tree_contents(&local.join("lock"), &plan.per_clone.locks)?;
    copy_tree_contents(&local.join("plugins"), &plan.per_clone.plugins_auth)?;
    Ok(())
}

/// Recursively copy entries from `src` into `dst`. A non-existent
/// `src` is a no-op; an entry that already exists at `dst` is left
/// alone (idempotent re-runs).
fn copy_tree_contents(src: &Path, dst: &Path) -> Result<()> {
    let Ok(entries) = fs::read_dir(src) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let from = entry.path();
        let name = entry.file_name();
        let to = dst.join(&name);
        if to.exists() {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            fs::create_dir_all(&to)?;
            copy_tree_contents(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

pub(super) fn move_worktrees(root: &Path, plan: &MigrationPlan) -> Result<()> {
    let src_root = root.join(".balls-worktrees");
    if !src_root.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(&plan.per_clone.worktrees)?;
    for entry in fs::read_dir(&src_root)?.flatten() {
        let from = entry.path();
        if !from.is_dir() {
            continue;
        }
        // No `to.exists()` guard: `git worktree move` fails fast if the
        // target already exists. A partial-migration retry shouldn't
        // silently leave the worktree in `.balls-worktrees/` for
        // `cleanup_on_main` to `rm -rf` (which would break the git
        // worktree registry) — better to surface the conflict.
        let to = plan.per_clone.worktrees.join(entry.file_name());
        git::clean_git_command(root)
            .args(["worktree", "move"])
            .arg(&from)
            .arg(&to)
            .status()
            .map_err(|e| BallError::Git(format!("git worktree move: {e}")))?;
    }
    Ok(())
}

pub(super) fn cleanup_on_main(root: &Path) -> Result<()> {
    let balls_in_index =
        !git::run_git_in(root, &["ls-files", "--", ".balls"])?.stdout.is_empty();
    if balls_in_index {
        git::run_git_in(root, &["rm", "-r", "-f", "-q", ".balls"])?;
    }
    let dot_balls = root.join(".balls");
    if dot_balls.exists() {
        let _ = fs::remove_dir_all(&dot_balls);
    }
    let worktrees_local = root.join(".balls-worktrees");
    if worktrees_local.exists() {
        let _ = fs::remove_dir_all(&worktrees_local);
    }
    strip_balls_gitignore(root)?;
    if git::has_staged_changes(root)? {
        git::git_commit(root, super::MIGRATION_COMMIT)?;
    }
    Ok(())
}

/// Drop every `balls`-shaped line from `.gitignore`. Matches the
/// runtime-path entries `bl init` writes (`.balls/state-repo`,
/// `.balls/local`, `.balls/tasks`, `.balls/project.json`,
/// `.balls/plugins`, `.balls/code-refs`, `.balls-worktrees`) plus
/// any plain `.balls` line.
fn strip_balls_gitignore(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    if !path.exists() {
        return Ok(());
    }
    let original = fs::read_to_string(&path)?;
    let kept: Vec<&str> = original
        .lines()
        .filter(|line| !is_balls_gitignore_line(line.trim()))
        .collect();
    let mut rewritten = kept.join("\n");
    if !rewritten.is_empty() && !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    if rewritten.trim().is_empty() {
        git::run_git_in(root, &["rm", "-q", "--", ".gitignore"])?;
    } else if rewritten != original {
        fs::write(&path, rewritten)?;
        git::git_add(root, &[Path::new(".gitignore")])?;
    }
    Ok(())
}

fn is_balls_gitignore_line(trimmed: &str) -> bool {
    trimmed == ".balls"
        || trimmed.starts_with(".balls/")
        || trimmed == ".balls-worktrees"
        || trimmed.starts_with(".balls-worktrees/")
}

#[cfg(test)]
#[path = "steps_tests.rs"]
mod tests;
