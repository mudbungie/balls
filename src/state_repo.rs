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
//! Hard-fail on first-time unreachable hub (bl-dcd3). A balls client
//! with `master_url` set is a pure pointer to a shared hub: if first
//! materialization can't reach the hub, the only safe outcome is to
//! stop. Silently dropping to a local orphan would let the user
//! accumulate task changes that diverge from the rest of the team —
//! the exact failure mode the cross-repo model exists to prevent.
//! A pre-existing local clone (warm cache) keeps working from cache
//! when offline; only the first-time-materialization path is fatal.

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
/// `origin` is `url`, with `balls/tasks` checked out. Idempotent.
///
/// **Hard-fail on first-time unreachable hub** (bl-dcd3). When the
/// local clone has not yet seen the hub's `balls/tasks` (no warm
/// cache) and a fetch from `url` fails, returns `Err` naming the URL,
/// the underlying fetch failure, and the three resolution paths. The
/// just-initialized scaffold is torn down so the next invocation
/// remains a clean first-time attempt rather than a half-built cache.
///
/// **Soft-fail on warm-cache offline.** When `balls/tasks` already
/// exists locally from a prior successful materialization, an offline
/// fetch is acceptable — the user works from cache, mirroring normal
/// git semantics. A note is printed and the call returns `Ok`.
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

    let (online, fetch_err) = fetch_origin(&dir)?;
    let warm_cache = git_state::branch_exists(&dir, STATE_BRANCH);

    if !online && !warm_cache {
        if first_time {
            // Roll back the just-created scaffold so the next attempt
            // is a clean first-time, not a partial-cache soft-fail.
            let _ = fs::remove_dir_all(&dir);
        }
        return Err(unreachable_hub_err(url, &fetch_err));
    }

    if !warm_cache {
        if git_state::has_remote_branch(&dir, "origin", STATE_BRANCH) {
            git_state::create_tracking_branch(&dir, STATE_BRANCH, "origin")?;
        } else {
            git_state::create_orphan_branch(&dir, STATE_BRANCH, "balls state")?;
            // Best-effort first publish; a divergent hub rejects
            // (non-force) and we stay safe-but-unlinked.
            let _ = git::git_push(&dir, "origin", STATE_BRANCH);
        }
        checkout(&dir, STATE_BRANCH)?;
    }

    seed(&dir)?;
    // Expose .balls/state-repo/.balls/tasks at the convenience path
    // .balls/tasks (mirrors the legacy `worktree`-mode symlink). The
    // legacy path is created in setup_state_branch; the master_url path
    // bypasses that helper entirely, so without this call the README's
    // "ls/$EDITOR .balls/tasks" ergonomic is missing on master_url repos.
    crate::store_init::ensure_tasks_symlink(root, "state-repo/.balls/tasks")?;

    if !online {
        eprintln!(
            "note: could not reach state hub `{url}`; continuing from the \
             local cache. Re-run `bl prime` (or `bl sync`) once the hub \
             is reachable."
        );
    }
    Ok(dir)
}

/// Capture success and stderr from `git fetch origin` in `dir`. The
/// stderr is folded into the hard-fail diagnostic so the user can
/// distinguish "host unreachable" from "permission denied" from
/// "ref not found" without re-running git by hand. Spawn-level
/// failures propagate as `BallError::Git`, separate from the friendly
/// hub-unreachable path — those mean git itself is broken, not the
/// hub.
fn fetch_origin(dir: &Path) -> Result<(bool, String)> {
    let out = git::run_git_in(dir, &["fetch", "origin"])?;
    Ok(if out.status.success() {
        (true, String::new())
    } else {
        (false, String::from_utf8_lossy(&out.stderr).trim().to_string())
    })
}

fn unreachable_hub_err(url: &str, fetch_err: &str) -> BallError {
    BallError::Other(format!(
        "could not reach state hub `{url}`\n  underlying error: {fetch_err}\n  \
         A balls master_url is required for first-time setup — \
         silently dropping to a local orphan would let your task \
         changes drift away from the rest of the project. Options:\n  \
         - Fix access (credentials, VPN, URL typo) and re-run.\n  \
         - Edit .balls/config.json to point master_url at a hub you can reach.\n  \
         - Run `bl remaster --detach` to drop the hub link and work standalone."
    ))
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
