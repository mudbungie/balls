//! `bl remaster` — re-point this repo's task state branch and
//! reconcile, or `--detach` back to standalone. The reconcile/detach
//! mechanics live in `balls::remaster`; this is arg handling, the
//! state_remote write (per-clone by default, committed with
//! `--commit`), and user-facing messaging.
//!
//! bl-ffb4: `--commit <URL>` auto-routes a URL-shaped target to the
//! new `master_url` field (committed hub URL + balls-owned checkout)
//! instead of the legacy `state_remote` name field. A bare remote
//! name still writes `state_remote` — old single-repo-shared-hub
//! setups keep working byte-identically.

use super::discover;
use balls::config::Config;
use balls::error::{BallError, Result};
use balls::policy::LocalConfig;
use balls::remaster::{self, Reconciled};
use balls::store::Store;
use balls::{git, gitignore, state_repo};
use std::path::Path;

pub fn cmd_remaster(target: Option<String>, commit: bool, detach: bool) -> Result<()> {
    if detach && target.is_some() {
        return Err(BallError::Other(
            "remaster --detach takes no TARGET (it goes standalone)".into(),
        ));
    }

    // Detach must work offline (bl-dcd3). When `master_url` is set but
    // the state-repo never materialized — an unreachable hub blocking
    // first-time setup — `discover()` re-hits the same hard-fail, so
    // the warm detach path can't run. Try the cold path first; it
    // returns Ok(false) when the warm path is the right answer.
    if detach {
        let cwd = std::env::current_dir()?;
        if remaster::try_cold_detach(&cwd)? {
            println!(
                "detached (offline): cleared master_url; initialized a fresh \
                 local task store at .balls/worktree/. Your tasks are not \
                 shared with any hub yet."
            );
            return Ok(());
        }
    }

    let store = discover()?;
    if store.no_git || store.stealth {
        return Err(BallError::Other(
            "remaster requires a non-stealth git-backed repo".into(),
        ));
    }

    if detach {
        // Captured before `detach` swaps the symlink for a real dir:
        // it tells us whether the federated git hygiene needs reversing.
        let was_federated = store.root.join(".balls/plugins").is_symlink();
        remaster::detach(&store)?;
        set_local_state_remote(&store, "origin")?;
        clear_master_url(&store)?;
        if was_federated {
            detach_gitignore_hygiene(&store)?;
        }
        println!(
            "detached: balls/tasks re-rooted as a standalone local store; \
             state_remote cleared to `origin`"
        );
        return Ok(());
    }

    let target = target.ok_or_else(|| {
        BallError::Other("remaster needs a TARGET remote (or use --detach)".into())
    })?;

    // URL-shaped target: route to the new master_url + balls-owned
    // checkout path. Materialize the state-repo *before* reconcile so
    // the shared `remaster::reconcile` plumbing sees a real
    // `origin/balls/tasks` to fast-forward onto (bl-ffb4).
    if state_repo::looks_like_url(&target) {
        return commit_master_url(&store, &target, commit);
    }

    // Reconcile first: on failure the link is untouched and the
    // command is safe to retry.
    let outcome = remaster::reconcile(&store, &target)?;

    if commit {
        let path = store.config_path();
        let mut cfg = Config::load(&path)?;
        cfg.state_remote = Some(target.clone());
        cfg.save(&path)?;
        println!(
            "wrote state_remote=`{target}` to committed .balls/config.json \
             — commit it to share the project link"
        );
    } else {
        set_local_state_remote(&store, &target)?;
        println!("set per-clone state_remote=`{target}` (.balls/local/config.json)");
    }

    match outcome {
        Reconciled::AlreadyUpToDate => {
            println!("already up to date with `{target}`");
        }
        Reconciled::Joined { replayed, renamed } => {
            println!(
                "joined `{target}`: {replayed} task(s) replayed, {renamed} renamed"
            );
        }
    }
    Ok(())
}

fn set_local_state_remote(store: &Store, remote: &str) -> Result<()> {
    let mut local = LocalConfig::load(store)?.unwrap_or_default();
    local.state_remote = Some(remote.to_string());
    local.save(store)
}

/// URL-shaped remaster target: materialize the balls-owned state-repo
/// and (when `--commit`) write the URL to committed config. Without
/// `--commit` we still materialize so the local checkout is usable
/// immediately; the per-clone link is the materialized state-repo
/// itself, no parallel config field needed (the legacy per-clone
/// override is for the *name* field).
fn commit_master_url(store: &Store, url: &str, commit: bool) -> Result<()> {
    state_repo::ensure(&store.root, url)?;
    if commit {
        let path = store.config_path();
        let mut cfg = Config::load(&path)?;
        cfg.master_url = Some(url.to_string());
        // Clearing legacy `state_remote` on commit removes the
        // ambiguity over which knob is authoritative — a fresh clone
        // would otherwise still try to resolve the name field too.
        cfg.state_remote = None;
        cfg.save(&path)?;
        commit_federated_flip(store, url)?;
    } else {
        println!(
            "materialized .balls/state-repo/ from `{url}` (per-clone, \
             not committed). Re-run with --commit to share the link."
        );
    }
    Ok(())
}

/// bl-ebae: leave the federated flip with a clean `git status`.
/// `state_repo::ensure` already swapped `.balls/plugins/` for a symlink
/// into the hub clone; here we (1) gitignore the balls-owned federated
/// paths so a careless `git add -A` can't bake them into the shared
/// repo, (2) drop the now-symlink-shadowed `.gitkeep` from the index,
/// and (3) commit the whole transition alongside the config change.
fn commit_federated_flip(store: &Store, url: &str) -> Result<()> {
    gitignore::ensure_main_gitignore(&store.root, false, true)?;
    git::git_rm_cached(&store.root, &[Path::new(".balls/plugins/.gitkeep")])?;
    git::git_add(
        &store.root,
        &[Path::new(".balls/config.json"), Path::new(".gitignore")],
    )?;
    git::git_commit(&store.root, "balls: remaster to federated hub")?;
    println!(
        "remastered to federated hub `{url}`: master_url committed, \
         .balls/state-repo + .balls/plugins gitignored"
    );
    Ok(())
}

/// bl-ebae: reverse `commit_federated_flip`. Detach turned the plugins
/// symlink back into a real directory (`remaster::detach`), so it must
/// leave the ignore list and its restored contents be re-tracked. The
/// whole standalone shape is committed so detach, too, leaves a clean
/// tree. `.balls/state-repo` stays ignored — the clone is still on disk.
fn detach_gitignore_hygiene(store: &Store) -> Result<()> {
    gitignore::remove_federated_entries(&store.root)?;
    git::git_add(
        &store.root,
        &[
            Path::new(".balls/plugins"),
            Path::new(".balls/config.json"),
            Path::new(".gitignore"),
        ],
    )?;
    git::git_commit(&store.root, "balls: remaster --detach to standalone")?;
    Ok(())
}

/// Detach also clears any committed `master_url` so the project drops
/// the hub link cleanly. No-op when the field was never set.
fn clear_master_url(store: &Store) -> Result<()> {
    let path = store.config_path();
    let mut cfg = Config::load(&path)?;
    if cfg.master_url.is_some() {
        cfg.master_url = None;
        cfg.save(&path)?;
    }
    Ok(())
}
