//! XDG `bl init --bare` per [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md)
//! §3, §4, §5 (Phase 1B-7, bl-be70).
//!
//! Bare-clone bootstrap that materializes the XDG layout instead of
//! the loose `.balls/` under the bare clone. Inputs:
//!
//! - `source` — a git URL or path whose `balls/tasks` branch carries a
//!   balls-initialized project (`bl init` was run in some working
//!   clone and pushed first).
//! - `clone_dir` — the target bare clone path; its identity for the
//!   XDG layout's per-clone tree is its canonical absolute filesystem
//!   path (§4 `<nested-clone-path>`).
//!
//! Outputs:
//!
//! - `<clone_dir>/.git` — the bare gitdir cloned from `source`, with
//!   the standard `+refs/heads/*:refs/remotes/origin/*` refspec wired.
//! - `~/.local/state/balls/trackers/<enc-source>/balls%2Ftasks/` — a
//!   single-branch clone of the source's `balls/tasks`.
//! - `~/.local/state/balls/{claims,locks,plugins-auth}/<nested>/` —
//!   per-clone tree under the bare clone's nested path.
//!
//! Idempotent: a present bare gitdir is reused (a non-bare one is
//! refused, never clobbered); a warm tracker checkout fetches origin
//! to refresh; per-clone dirs are `create_dir_all`'d.

use crate::encoding::{canonicalize_origin, nested_clone_path, percent_encode_component};
use crate::error::{BallError, Result};
use crate::git;
use crate::store::Store;
use crate::xdg_paths::{own_tracker_checkout, PerClonePaths, XdgBases};
use std::fs;
use std::path::{Path, PathBuf};

/// SPEC §5 bootstrap branch name. Same constant as `store_init_xdg`;
/// inlined rather than re-exported because the two callers' lifetimes
/// of the binding are independent.
const BOOTSTRAP_BRANCH: &str = "balls/tasks";

/// XDG bare init entry point. Errors loudly when `HOME` is unset, the
/// source has no `balls/tasks` branch, or the target path already
/// holds a non-bare `.git`.
pub fn init(source: &str, clone_dir: &Path) -> Result<Store> {
    let bases = XdgBases::from_env()
        .ok_or_else(|| BallError::Other("HOME must be set for XDG bl init --bare".into()))?;
    let clone_dir = bare_clone(source, clone_dir)?;
    let enc_origin = percent_encode_component(&canonicalize_origin(source));
    let tracker = own_tracker_checkout(&bases, &enc_origin);
    materialize_tracker_from_source(&tracker, source)?;
    ensure_per_clone(&bases, &clone_dir)?;
    Store::discover(&clone_dir)
}

/// Bare-clone `source` into `<clone_dir>/.git`. Reuses an existing
/// bare gitdir; refuses to clobber a non-bare one.
fn bare_clone(source: &str, clone_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(clone_dir)?;
    let clone_dir = fs::canonicalize(clone_dir).unwrap_or_else(|_| clone_dir.to_path_buf());
    let gitdir = clone_dir.join(".git");
    if gitdir.exists() {
        if !crate::bare_squash::is_bare_repo(&clone_dir).unwrap_or(false) {
            return Err(BallError::Other(format!(
                "{} exists and is not a bare repo; refusing to clobber it",
                gitdir.display()
            )));
        }
    } else {
        git::git_clone_bare(source, &gitdir)?;
    }
    git::git_config_set(
        &clone_dir,
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    )?;
    let _ = git::git_fetch(&clone_dir, "origin");
    git::git_ensure_user(&clone_dir)?;
    Ok(clone_dir)
}

/// Clone the source's `balls/tasks` branch into the XDG tracker
/// checkout. The source MUST already carry the branch — there is no
/// working tree to seed it from. A warm checkout fetches origin
/// instead and re-aligns the remote URL.
fn materialize_tracker_from_source(tracker: &Path, source: &str) -> Result<()> {
    if tracker.join(".git").exists() {
        git::git_config_set(tracker, "remote.origin.url", source)?;
        let _ = git::git_fetch(tracker, "origin");
        return Ok(());
    }
    fs::create_dir_all(tracker.parent().expect("trackers/<enc> has parent"))?;
    let out = std::process::Command::new("git")
        .args(["clone", "-q", "--single-branch", "--branch", BOOTSTRAP_BRANCH])
        .arg(source)
        .arg(tracker)
        .output()
        .map_err(|e| BallError::Other(format!("git clone failed to spawn: {e}")))?;
    if !out.status.success() {
        return Err(BallError::Other(format!(
            "source has no `{BOOTSTRAP_BRANCH}` branch — run `bl init` in a \
             working clone and push first (README bootstrap step 1): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    git::git_ensure_user(tracker)?;
    Ok(())
}

/// Materialize the per-clone catch-all dirs under the bare clone's
/// nested path. Worktrees are materialized lazily by `bl claim`.
fn ensure_per_clone(bases: &XdgBases, clone_dir: &Path) -> Result<()> {
    let nested = nested_clone_path(clone_dir);
    let per = PerClonePaths::new(bases, &nested);
    for d in [&per.claims, &per.locks, &per.plugins_auth] {
        fs::create_dir_all(d)?;
    }
    Ok(())
}
