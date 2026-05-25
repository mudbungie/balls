//! `bl migrate` — pre-XDG → nested XDG conversion per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §11.1.
//!
//! Idempotent, refuses-if-dirty, single transactional shape per clone:
//! all per-file moves staged into a state-branch commit; the on-`main`
//! cleanup commit lands last, after the XDG tree is verified
//! materializable. That on-`main` commit is the final balls-attributed
//! commit on `main` for the lifetime of the clone (SPEC §11.1).
//!
//! Scope reduction vs the bl-717e ball description: the §11.2
//! hashed-XDG path (the bl-fc50 layout) was reverted in bl-77cb before
//! any release shipped — no real clone carries it. The "bl-ed32
//! two-file → three-file" path the ball description named was a
//! SPEC-only artifact (no binary ever wrote `workspace.json`).
//! Combined-history requires the hashed-XDG path to have shipped, so
//! it cannot exist in practice. The omission is intentional, not
//! forgotten; a future ball can add defense-in-depth for a
//! never-shipped layout if one is ever observed in the wild.

use crate::config::Config;
use crate::encoding::{canonicalize_origin, nested_clone_path, percent_encode_component};
use crate::error::{BallError, Result};
use crate::repo_url;
use crate::xdg_paths::{own_tracker_checkout, PerClonePaths, XdgBases};
use crate::{git, store_paths};
use std::fs;
use std::path::{Path, PathBuf};

// Step implementations live in `migrate_steps` so this module stays
// under the 300-line cap (decomposition convention). Both files are
// kept tightly coupled — the `MigrationPlan` here is the single
// argument every step takes.
pub(crate) mod steps;

/// Tag applied to the on-`main` migration commit. SPEC §11.1: the
/// migration commit is "the final balls-attributed commit on `main`
/// for the lifetime of the clone." The `[bl-717e]` tag attributes the
/// commit to the ball that introduced the migrate command; future
/// re-runs (e.g. after a manual revert) land the same well-known
/// subject.
pub const MIGRATION_COMMIT: &str = "balls: migrate to XDG layout [bl-717e]";

/// Tag for the state-branch commit that writes `repo.json` (and an
/// optional `tracker.json` redirect). Lands before the on-main
/// cleanup so the SPEC §11.1 ordering ("state-branch first, on-main
/// cleanup last") survives a partial-migration crash.
pub const STATE_COMMIT: &str = "balls: migrate clone config to repo.json [bl-717e]";

/// The human-readable diagnostic returned to the CLI. Multiple lines
/// each emitted on their own `println!` so the CLI wrapper stays a
/// pure pass-through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report(pub Vec<String>);

impl Report {
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

/// Run the migration starting from `from` (typically the current
/// working directory). Locates the clone root, detects layout, and
/// either runs the conversion or reports "nothing to migrate."
pub fn run(from: &Path) -> Result<Report> {
    let root = locate_clone_root(from)?;
    match detect(&root)? {
        Detection::AlreadyMigrated => Ok(Report(vec![
            "nothing to migrate; clone is on the XDG layout".into(),
        ])),
        Detection::NotBalls => Err(BallError::Other(
            "no balls state to migrate (no `.balls/config.json` and no XDG state \
             materialized). Run `bl init` first."
                .into(),
        )),
        Detection::Legacy => execute(&root),
    }
}

/// Walk from `from` up to a clone root: the directory holding either
/// the legacy `.balls/config.json` or a `.git/` (post-migration).
/// `bl migrate` may be invoked from a task worktree, so the walk has
/// to climb out the same way `Store::discover` does.
fn locate_clone_root(from: &Path) -> Result<PathBuf> {
    let common_dir = git::git_common_dir(from)?;
    store_paths::find_main_root(&common_dir)
}

enum Detection {
    Legacy,
    AlreadyMigrated,
    NotBalls,
}

fn detect(root: &Path) -> Result<Detection> {
    if root.join(".balls/config.json").exists() {
        return Ok(Detection::Legacy);
    }
    let bases = XdgBases::from_env()
        .ok_or_else(|| BallError::Other("HOME is not set; cannot resolve XDG paths".into()))?;
    let Some(url) = repo_url::origin_url(root) else {
        return Ok(Detection::NotBalls);
    };
    let enc = percent_encode_component(&canonicalize_origin(&url));
    let checkout = own_tracker_checkout(&bases, &enc);
    if checkout.join(".git").exists() {
        Ok(Detection::AlreadyMigrated)
    } else {
        Ok(Detection::NotBalls)
    }
}

/// Drive the full migration. Ordering is load-bearing (SPEC §11.1):
/// state-branch writes first, on-`main` cleanup last so a crash leaves
/// a re-runnable half-state rather than a clone with `.balls/` gone
/// but XDG not materialized.
fn execute(root: &Path) -> Result<Report> {
    let bases = XdgBases::from_env()
        .ok_or_else(|| BallError::Other("HOME is not set; cannot resolve XDG paths".into()))?;
    let origin_url = repo_url::origin_url(root).ok_or_else(|| {
        BallError::Other(
            "no `origin` remote configured; bl migrate cannot derive the XDG \
             tracker path without one (SPEC-clone-layout §4)"
                .into(),
        )
    })?;

    refuse_if_dirty(root)?;

    let plan = MigrationPlan::compute(root, &bases, &origin_url);
    let legacy_cfg = Config::load(&root.join(".balls/config.json"))?;
    let mut notes = Vec::new();

    steps::materialize_tracker_checkout(root, &plan, &origin_url)?;
    steps::write_xdg_config_files(&plan, &legacy_cfg, &mut notes)?;
    steps::move_per_clone_state(root, &plan)?;
    steps::move_worktrees(root, &plan)?;
    steps::cleanup_on_main(root)?;

    notes.push(format!(
        "migrated to XDG layout: tracker checkout at {}, per-clone state at {}",
        plan.tracker_checkout.display(),
        plan.per_clone
            .worktrees
            .parent()
            .map(Path::display)
            .map(|d| d.to_string())
            .unwrap_or_default(),
    ));
    Ok(Report(notes))
}

/// Resolved XDG destinations for one clone. Computed once at the top
/// of `execute` and threaded through every step so the encoding work
/// happens in one place.
pub(crate) struct MigrationPlan {
    pub tracker_checkout: PathBuf,
    pub per_clone: PerClonePaths,
}

impl MigrationPlan {
    fn compute(root: &Path, bases: &XdgBases, origin_url: &str) -> Self {
        let enc_origin = percent_encode_component(&canonicalize_origin(origin_url));
        let tracker_checkout = own_tracker_checkout(bases, &enc_origin);
        let nested = nested_clone_path(root);
        let per_clone = PerClonePaths::new(bases, &nested);
        Self { tracker_checkout, per_clone }
    }
}

/// Refuse the migration if the clone has uncommitted changes on
/// `main` or in any active task worktree (SPEC §11.1 + §11.3). The
/// migration moves worktrees with `git worktree move` and removes
/// `.balls/` on `main`; either step would silently drop in-flight
/// edits if we proceeded over dirty state.
fn refuse_if_dirty(root: &Path) -> Result<()> {
    if git::has_uncommitted_changes(root)? {
        return Err(BallError::Other(format!(
            "`{}` has uncommitted changes on main; commit, stash, or \
             discard them and re-run `bl migrate`",
            root.display()
        )));
    }
    let worktrees_root = root.join(".balls-worktrees");
    let Ok(entries) = fs::read_dir(&worktrees_root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if git::has_uncommitted_changes(&path)? {
            return Err(BallError::Other(format!(
                "task worktree `{}` has uncommitted changes; commit or \
                 `bl drop` it before running `bl migrate`",
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
