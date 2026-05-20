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
use balls::state_repo;
use balls::store::Store;

pub fn cmd_remaster(target: Option<String>, commit: bool, detach: bool) -> Result<()> {
    let store = discover()?;
    if store.no_git || store.stealth {
        return Err(BallError::Other(
            "remaster requires a non-stealth git-backed repo".into(),
        ));
    }

    if detach {
        if target.is_some() {
            return Err(BallError::Other(
                "remaster --detach takes no TARGET (it goes standalone)".into(),
            ));
        }
        remaster::detach(&store)?;
        set_local_state_remote(&store, "origin")?;
        clear_master_url(&store)?;
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
        println!(
            "wrote master_url=`{url}` to committed .balls/config.json — \
             balls owns .balls/state-repo/; commit the config to share \
             the project link"
        );
    } else {
        println!(
            "materialized .balls/state-repo/ from `{url}` (per-clone, \
             not committed). Re-run with --commit to share the link."
        );
    }
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
