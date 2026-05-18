//! `bl remaster` — re-point this repo's task state branch and
//! reconcile, or `--detach` back to standalone. The reconcile/detach
//! mechanics live in `balls::remaster`; this is arg handling, the
//! state_remote write (per-clone by default, committed with
//! `--commit`), and user-facing messaging.

use super::discover;
use balls::config::Config;
use balls::error::{BallError, Result};
use balls::policy::LocalConfig;
use balls::remaster::{self, Reconciled};
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
        println!(
            "detached: balls/tasks re-rooted as a standalone local store; \
             state_remote cleared to `origin`"
        );
        return Ok(());
    }

    let target = target.ok_or_else(|| {
        BallError::Other("remaster needs a TARGET remote (or use --detach)".into())
    })?;

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
