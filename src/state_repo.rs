//! Balls-owned state checkout under `.balls/state-repo/` (bl-ffb4).
//!
//! The legacy `state_remote` (name) field stores a project-side git
//! remote, which means a fresh `git clone` of the project can't resolve
//! the hub — the remote name lives in per-clone `.git/config` and is
//! not tracked. `master_url` closes that gap by storing the hub URL
//! directly in committed config and materializing balls's own git
//! clone here, completely separate from the project's `.git/`. Every
//! state-branch op routes through this clone via the existing
//! `state_worktree_dir()` seam; the project's git is untouched.
//!
//! Idempotent: re-running `ensure` against an already-materialized
//! state-repo just fetches origin. Safe-but-unlinked guarantees from
//! `bl init` carry through: if the URL is unreachable we surface a
//! note and fall back to a local orphan store, never a destructive
//! force-push or reset.

use crate::error::{BallError, Result};
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

/// Relative path (from the repo root) of the balls-owned state clone.
/// Distinct from `STATE_WORKTREE_REL` (`.balls/worktree`, the legacy
/// project-worktree path) so a config can flip between models without
/// the two layouts stomping on each other.
pub(crate) const STATE_REPO_REL: &str = ".balls/state-repo";

const STATE_BRANCH: &str = "balls/tasks";

/// Materialize `.balls/state-repo/` as a balls-owned git clone whose
/// `origin` is `url`, with `balls/tasks` checked out. Idempotent and
/// non-destructive: a pre-existing state-repo is reused; an unreachable
/// `url` falls back to an isolated local orphan store (mirrors
/// `setup_state_branch`'s safe-but-unlinked behavior, bl-8e8f).
pub fn ensure(root: &Path, url: &str) -> Result<PathBuf> {
    let dir = root.join(STATE_REPO_REL);
    let first_time = !dir.join(".git").exists();
    if first_time {
        init_with_origin(&dir, url)?;
    } else {
        // Origin may have been re-pointed by a later remaster --commit;
        // keep the recorded URL authoritative.
        let _ = git::git_config_set(&dir, "remote.origin.url", url);
    }
    git::git_ensure_user(&dir)?;

    let online = git::git_fetch(&dir, "origin").unwrap_or(false);

    if !git_state::branch_exists(&dir, STATE_BRANCH) {
        if online && git_state::has_remote_branch(&dir, "origin", STATE_BRANCH) {
            git_state::create_tracking_branch(&dir, STATE_BRANCH, "origin")?;
            checkout(&dir, STATE_BRANCH)?;
        } else {
            git_state::create_orphan_branch(&dir, STATE_BRANCH, "balls state")?;
            checkout(&dir, STATE_BRANCH)?;
            if online {
                // Best-effort first publish; a divergent hub rejects
                // (non-force) and we stay safe-but-unlinked.
                let _ = git::git_push(&dir, "origin", STATE_BRANCH);
            }
        }
    } else if online && git_state::has_remote_branch(&dir, "origin", STATE_BRANCH) {
        // Local branch exists; nothing to do here — `bl sync` is the
        // path that fast-forwards/merges further hub history.
    }

    seed(&dir)?;

    if !online {
        eprintln!(
            "note: could not reach state hub `{url}`. Created an isolated local \
             task store at .balls/state-repo/ — your tasks are not shared yet. \
             Re-run `bl prime` (or `bl sync`) once the hub is reachable."
        );
    }
    Ok(dir)
}

fn init_with_origin(dir: &Path, url: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    // `git init` with the state branch as initial branch keeps the
    // first orphan commit on the right ref without a separate checkout.
    run_at(
        dir.parent().unwrap_or(dir),
        &[
            "init",
            "-q",
            "--initial-branch",
            STATE_BRANCH,
            &dir.to_string_lossy(),
        ],
    )?;
    run_at(dir, &["remote", "add", "origin", url])
}

fn checkout(dir: &Path, branch: &str) -> Result<()> {
    run_at(dir, &["checkout", "-q", branch])
}

fn run_at(dir: &Path, args: &[&str]) -> Result<()> {
    let status = git::clean_git_command(dir)
        .args(args)
        .status()
        .map_err(|e| BallError::Git(format!("git {}: {e}", args.join(" "))))?;
    if !status.success() {
        return Err(BallError::Git(format!(
            "git {} exited with {status}",
            args.join(" ")
        )));
    }
    Ok(())
}

/// Seed `.balls/tasks/` scaffolding (mirrors `setup_state_branch`'s
/// `seed_state_worktree`). Pulled out here so `state_repo::ensure` can
/// stay self-contained — `store_init`'s helper is private to that
/// module and serving a different code path.
fn seed(state_repo: &Path) -> Result<()> {
    let tasks = state_repo.join(".balls/tasks");
    fs::create_dir_all(&tasks)?;

    let attrs = tasks.join(".gitattributes");
    let attrs_line = "*.notes.jsonl merge=union\n";
    let need_attrs = match fs::read_to_string(&attrs) {
        Ok(s) => !s.contains("*.notes.jsonl merge=union"),
        Err(_) => true,
    };
    if need_attrs {
        fs::write(&attrs, attrs_line)?;
    }

    let keep = tasks.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }

    if git::has_uncommitted_changes(state_repo)? {
        git::git_add_all(state_repo)?;
        git::git_commit(state_repo, "balls: seed state branch")?;
    }
    Ok(())
}

/// Detect URL-shaped remaster targets so `bl remaster --commit <X>` can
/// auto-route a URL to `master_url` and a bare name to legacy
/// `state_remote`. Conservative: anything ambiguous stays a name.
pub fn looks_like_url(s: &str) -> bool {
    s.contains("://")
        || s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("../")
        || ssh_shorthand(s)
}

fn ssh_shorthand(s: &str) -> bool {
    // `user@host:path` — a single colon, non-empty user/host/path, and
    // not also a URL scheme. Conservative on `host:port`-only strings
    // to avoid false positives against `origin:1234` style names.
    let Some((host, path)) = s.split_once(':') else {
        return false;
    };
    !host.is_empty()
        && !path.is_empty()
        && host.contains('@')
        && !path.contains(' ')
        && !s.contains("://")
}

#[cfg(test)]
#[path = "state_repo_tests.rs"]
mod tests;
