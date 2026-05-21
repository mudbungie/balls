//! `bl remaster` — re-point the tracker address, or detach to standalone.
//!
//! `bl remaster <url>` writes `state_url` into `.balls/config.json`
//! and reconciles local-only tasks onto the new tracker (mechanics in
//! `balls::remaster`). `--detach` clears the address and re-roots the
//! state checkout. There is one address and one mechanism — no mode,
//! no federated flip, no transplant.

use super::discover;
use balls::config::Config;
use balls::error::{BallError, Result};
use balls::remaster::{self, Reconciled};
use balls::{git, git_state, state_repo};
use std::path::{Path, PathBuf};

pub fn cmd_remaster(target: Option<String>, commit: bool, detach: bool) -> Result<()> {
    if detach && target.is_some() {
        return Err(BallError::Other(
            "remaster --detach takes no TARGET (it goes standalone)".into(),
        ));
    }
    let cwd = std::env::current_dir()?;
    // Resolve the root with git plumbing only — `discover` hard-fails
    // against an unreachable explicit tracker, and `--detach` must
    // work in exactly that state.
    let root = project_root(&cwd)?;
    if !root.join(".balls/config.json").exists() {
        return Err(BallError::Other(
            "not a balls workspace — run `bl init` before `bl remaster`".into(),
        ));
    }
    if detach {
        return detach_path(&root);
    }
    let target = target.ok_or_else(|| {
        BallError::Other("remaster needs a TARGET tracker URL (or use --detach)".into())
    })?;
    remaster_to(&root, &target, commit)
}

/// `bl remaster <url>`: reconcile onto the new tracker, then record
/// the address. Reconcile runs first so a failed join leaves the
/// committed address untouched.
fn remaster_to(root: &Path, target: &str, commit: bool) -> Result<()> {
    let url = resolve_target(root, target);
    let store = discover()?;
    if store.no_git || store.stealth {
        return Err(BallError::Other(
            "remaster requires a non-stealth git-backed repo".into(),
        ));
    }
    let outcome = remaster::reconcile(&store, &url)?;
    write_address(root, Some(&url), commit)?;
    match outcome {
        Reconciled::Seeded => println!("remastered to `{url}` — seeded a fresh tracker"),
        Reconciled::AlreadyUpToDate => println!("already up to date with `{url}`"),
        Reconciled::Joined { replayed, renamed } => {
            println!("joined `{url}`: {replayed} task(s) replayed, {renamed} renamed");
        }
    }
    Ok(())
}

/// `bl remaster --detach`: clear the address (reverting to the
/// implicit code `origin`) and re-root the state checkout. Offline.
fn detach_path(root: &Path) -> Result<()> {
    write_address(root, None, true)?;
    remaster::detach(root)?;
    println!(
        "detached: cleared the tracker address; .balls/state-repo re-rooted \
         as a standalone local store"
    );
    Ok(())
}

/// Write (or clear, when `url` is `None`) the tracker address in
/// `.balls/config.json`, migrating the legacy `master_url` /
/// `state_remote` fields away. `commit` also stages and commits it.
fn write_address(root: &Path, url: Option<&str>, commit: bool) -> Result<()> {
    let cfg_path = root.join(".balls/config.json");
    let mut cfg = Config::load(&cfg_path)?;
    cfg.state_url = url.map(str::to_string);
    if url.is_none() {
        cfg.state_branch = None;
    }
    cfg.master_url = None;
    cfg.state_remote = None;
    cfg.save(&cfg_path)?;
    // Retire the legacy pointer file once its content has folded in.
    let pointer = root.join(".balls/master.json");
    if pointer.exists() {
        std::fs::remove_file(&pointer)?;
    }
    if commit {
        git::git_add(root, &[Path::new(".balls/config.json")])?;
        let msg = if url.is_some() {
            "balls: remaster — set tracker address"
        } else {
            "balls: remaster --detach — standalone"
        };
        git::git_commit(root, msg)?;
    }
    Ok(())
}

/// A bare git-remote name is resolved to its URL; a URL/path is used
/// as-is. The address stored in `config.json` is always a URL.
fn resolve_target(root: &Path, target: &str) -> String {
    if state_repo::looks_like_url(target) {
        return target.to_string();
    }
    git_state::remote_url(root, target).unwrap_or_else(|| target.to_string())
}

/// Resolve the workspace root with git plumbing only.
fn project_root(from: &Path) -> Result<PathBuf> {
    let common = git::git_common_dir(from)?;
    let canon = std::fs::canonicalize(&common).unwrap_or(common);
    canon
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| BallError::Other("could not find the workspace root".into()))
}
