//! `bl remaster` — join a hub, re-point a legacy `state_remote`, or
//! detach back to standalone. Reconcile/detach mechanics live in
//! `balls::remaster`; the URL-shaped federation path delegates to
//! `balls::federate`. This module owns arg handling, the project-git
//! hygiene of the federated flip (gitignore + untrack), and messaging.
//!
//! bl-82a4: a URL target is the federation entry point and works on a
//! non-initted repo too. The committed federation artifact is the
//! `.balls/master.json` pointer; `.balls/config.json` + `.balls/plugins`
//! are gitignored symlinks recreated by `state_repo::ensure` (bl-ebae).

use super::discover;
use balls::error::{BallError, Result};
use balls::federate::{self, FederateReport};
use balls::master_pointer::MasterPointer;
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

    // URL-shaped target on a non-initted repo: bootstrap directly,
    // without `discover()` (which errors on a missing `.balls/`).
    if let Some(t) = target.as_deref() {
        if state_repo::looks_like_url(t) && !detach {
            let cwd = std::env::current_dir()?;
            if !cwd.join(".balls").exists() {
                return bootstrap_url(&cwd, t, commit);
            }
        }
    }

    let store = discover()?;
    if store.no_git || store.stealth {
        return Err(BallError::Other(
            "remaster requires a non-stealth git-backed repo".into(),
        ));
    }

    if detach {
        return detach_path(&store);
    }

    let target = target.ok_or_else(|| {
        BallError::Other("remaster needs a TARGET remote (or use --detach)".into())
    })?;

    if state_repo::looks_like_url(&target) {
        return federate_url(&store, &target, commit);
    }

    let outcome = remaster::reconcile(&store, &target)?;
    write_state_remote(&store, &target, commit)?;
    print_reconciled(&target, outcome);
    Ok(())
}

fn detach_path(store: &Store) -> Result<()> {
    // Captured before `detach`/`unfederate` undo the federated shape.
    let was_federated = federate::is_federated(&store.root);
    remaster::detach(store)?;
    federate::unfederate(&store.root)?;
    remaster::scrub_legacy_canonical(&store.root)?;
    set_local_state_remote(store, "origin")?;
    if was_federated {
        detach_gitignore_hygiene(store)?;
    }
    println!(
        "detached: balls/tasks re-rooted as a standalone local store; \
         master.json cleared, state_remote local override set to `origin`"
    );
    Ok(())
}

/// `bl remaster <url>` on an initialized repo. `--commit` runs the
/// federated flip and commits it; without it, only the per-clone
/// state-repo materializes.
fn federate_url(store: &Store, url: &str, commit: bool) -> Result<()> {
    if !commit {
        state_repo::ensure(&store.root, url)?;
        println!(
            "materialized .balls/state-repo/ from `{url}` (per-clone, \
             not committed). Re-run with --commit to share the link."
        );
        return Ok(());
    }
    if federate::is_federated(&store.root) {
        MasterPointer {
            master_url: Some(url.to_string()),
            state_remote: None,
        }
        .save(&store.root)?;
        println!("already federated to `{url}` (.balls/master.json refreshed)");
        return Ok(());
    }
    let report = federate::federate(&store.root, url)?;
    commit_federated_flip(&store.root, url)?;
    print_federation(url, &report);
    Ok(())
}

/// `bl remaster <url>` on a fresh git clone with no `.balls/`.
fn bootstrap_url(root: &Path, url: &str, commit: bool) -> Result<()> {
    if !commit {
        return Err(BallError::Other(
            "remaster <url> on a non-initted repo needs --commit (the \
             federation pointer must be tracked by git for `git clone` \
             to carry it)".into(),
        ));
    }
    let report = federate::bootstrap_non_initted(root, url)?;
    commit_federated_flip(root, url)?;
    print_federation(url, &report);
    Ok(())
}

/// Project-git hygiene for the federated flip (bl-ebae + bl-82a4):
/// gitignore the runtime sidecars, untrack the now-gitignored
/// canonical + plugins `.gitkeep`, and commit the `.balls/master.json`
/// pointer + `.gitignore` so the transition leaves a clean `git status`.
fn commit_federated_flip(root: &Path, url: &str) -> Result<()> {
    gitignore::ensure_main_gitignore(root, false, true)?;
    git::git_rm_cached(
        root,
        &[Path::new(".balls/config.json"), Path::new(".balls/plugins/.gitkeep")],
    )?;
    git::git_add(root, &[Path::new(".balls/master.json"), Path::new(".gitignore")])?;
    git::git_commit(root, "balls: remaster to federated hub")?;
    println!(
        "remastered to federated hub `{url}`: master.json committed, \
         .balls/config.json + .balls/plugins + .balls/state-repo gitignored"
    );
    Ok(())
}

/// Reverse `commit_federated_flip` on detach: drop the federated-only
/// gitignore entries (so `.balls/config.json` + `.balls/plugins`, now
/// real again, are re-tracked), untrack the removed pointer, and
/// commit the standalone shape.
fn detach_gitignore_hygiene(store: &Store) -> Result<()> {
    gitignore::remove_federated_entries(&store.root)?;
    git::git_rm_cached(&store.root, &[Path::new(".balls/master.json")])?;
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

fn write_state_remote(store: &Store, target: &str, commit: bool) -> Result<()> {
    if commit {
        let mut pointer = MasterPointer::load(&store.root)?;
        pointer.state_remote = Some(target.to_string());
        pointer.save(&store.root)?;
        println!(
            "wrote state_remote=`{target}` to committed .balls/master.json \
             — commit it to share the project link"
        );
    } else {
        set_local_state_remote(store, target)?;
        println!("set per-clone state_remote=`{target}` (.balls/local/config.json)");
    }
    Ok(())
}

fn set_local_state_remote(store: &Store, remote: &str) -> Result<()> {
    let mut local = LocalConfig::load(store)?.unwrap_or_default();
    local.state_remote = Some(remote.to_string());
    local.save(store)
}

fn print_federation(url: &str, report: &FederateReport) {
    println!("federated to `{url}` — .balls/state-repo/ owns task state");
    if !report.promoted_plugins.is_empty() {
        println!("  promoted plugins to hub: {}", report.promoted_plugins.join(", "));
    }
    if !report.discarded_plugins.is_empty() {
        println!(
            "  discarded project-side plugin entries (hub wins): {}",
            report.discarded_plugins.join(", ")
        );
    }
}

fn print_reconciled(target: &str, outcome: Reconciled) {
    match outcome {
        Reconciled::AlreadyUpToDate => println!("already up to date with `{target}`"),
        Reconciled::Joined { replayed, renamed } => {
            println!("joined `{target}`: {replayed} task(s) replayed, {renamed} renamed");
        }
    }
}
