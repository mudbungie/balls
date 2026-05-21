//! Balls-owned state checkout under `.balls/state-repo/` (bl-ffb4).
//!
//! `master_url` stores the hub URL in the committed `.balls/master.json`
//! pointer (bl-82a4) and materializes balls's own git clone here,
//! separate from the project's `.git/`. Every state-branch op routes
//! through this clone via the `state_worktree_dir()` seam.
//!
//! Hard-fail on first-time unreachable hub (bl-dcd3): silently dropping
//! to a local orphan would let task changes drift from the team. A
//! warm cache keeps working offline; only first-time materialization
//! is fatal.

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
/// Hard-fails first-time when the hub is unreachable (no warm cache),
/// tearing down the scaffold; soft-fails offline once a warm cache
/// exists (bl-dcd3).
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
    // bl-1098: parallel `.balls/plugins/` symlink so plugin config reads
    // resolve through the project root without any code-side branching
    // on master_url. Two parallel symlinks (a), not an umbrella path.
    ensure_plugins_symlink(root, "state-repo/.balls/plugins")?;
    // bl-82a4: same for `.balls/config.json` — the canonical config is
    // the hub's. A fresh `git clone` carries only `.balls/master.json`;
    // this materializes the symlink so `Config::load` resolves.
    ensure_config_symlink(root, "state-repo/.balls/config.json")?;

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
/// stderr is folded into the hard-fail diagnostic so the user can tell
/// "host unreachable" from "permission denied" from "ref not found".
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
         - Edit .balls/master.json to point master_url at a hub you can reach.\n  \
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
/// `seed_state_worktree`), kept here so `ensure` stays self-contained.
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

/// Materialize `.balls/plugins` as a symlink to `target` (relative to
/// `<root>/.balls/`) so per-plugin config files resolve through the
/// project root in federated mode (bl-1098). Idempotent; repoints a
/// stale symlink. A real `.balls/plugins/` is removed if `.gitkeep`-
/// only, refused if it carries config files (the migration is
/// `bl remaster`'s job).
pub(crate) fn ensure_plugins_symlink(root: &Path, target: &str) -> Result<()> {
    let link = root.join(".balls/plugins");
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(&link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(&link)?;
    } else if link.exists() {
        drop_placeholder_plugins_dir(&link)?;
    }
    std::os::unix::fs::symlink(&want, &link)?;
    Ok(())
}

/// Materialize `.balls/config.json` as a symlink to `target` (the
/// hub's canonical) — bl-82a4. Idempotent; repoints a stale symlink. A
/// *real* `.balls/config.json` is left untouched (standalone, or a
/// legacy federated repo `bl remaster` migrates). The case this
/// materializes is the fresh clone — no canonical, only the committed
/// `.balls/master.json`.
pub(crate) fn ensure_config_symlink(root: &Path, target: &str) -> Result<()> {
    let link = root.join(".balls/config.json");
    let want = PathBuf::from(target);
    if link.is_symlink() {
        if fs::read_link(&link).ok().as_deref() == Some(want.as_path()) {
            return Ok(());
        }
        fs::remove_file(&link)?;
    } else if link.exists() {
        return Ok(()); // real file: standalone or legacy — leave it
    }
    std::os::unix::fs::symlink(&want, &link)?;
    Ok(())
}

fn drop_placeholder_plugins_dir(dir: &Path) -> Result<()> {
    let real: Vec<String> = fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != ".gitkeep")
        .collect();
    if !real.is_empty() {
        return Err(BallError::Other(format!(
            "`.balls/plugins/` contains {real:?}; under master_url the hub is \
             authoritative (bl-a7d9). Move these into the hub's \
             `.balls/plugins/` and remove `.balls/plugins/` here, then retry."
        )));
    }
    fs::remove_dir_all(dir)?;
    Ok(())
}

/// Inverse of `ensure_plugins_symlink`: replace the symlink with a real
/// directory carrying the hub's plugin files at detach time, so the
/// new-standalone repo keeps its plugin config instead of losing it.
/// Idempotent when already a real dir.
pub(crate) fn restore_plugins_dir(root: &Path, state_repo_plugins: &Path) -> Result<()> {
    let link = root.join(".balls/plugins");
    if link.is_symlink() {
        fs::remove_file(&link)?;
    } else if link.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(&link)?;
    if let Ok(rd) = fs::read_dir(state_repo_plugins) {
        for entry in rd.flatten() {
            let from = entry.path();
            if from.is_file() {
                fs::copy(&from, link.join(entry.file_name()))?;
            }
        }
    }
    let keep = link.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "state_repo_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "state_repo_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "state_repo_plugins_tests.rs"]
mod plugins_tests;
