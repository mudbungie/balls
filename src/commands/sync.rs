//! sync, resolve, prime, repair — remote reconciliation and agent bootstrap.

use super::half_push::{detect_half_push, write_forget_half_push};
use super::sync_report::apply_sync_report;
use super::{default_identity, discover};
use balls::error::{BallError, Result};
use balls::store::Store;
use balls::task::{Status, Task};
use balls::{git, plugin, policy, ready, resolve, sync_resolve, worktree};
use std::fs;
use std::path::{Path, PathBuf};

pub fn cmd_sync(remote: String, task_filter: Option<String>) -> Result<()> {
    let store = discover()?;
    if !store.no_git && git::git_has_remote(&store.root, &remote) {
        sync_with_remote(&store, &remote)?;
    }
    let ident = default_identity();
    match plugin::dispatch_sync(&store, task_filter.as_deref(), &ident) {
        Ok(reports) => {
            for (plugin_name, report) in reports {
                apply_sync_report(&store, &plugin_name, &report);
            }
        }
        Err(e) => eprintln!("warning: plugin sync failed: {e}"),
    }
    eprintln!("sync complete");
    Ok(())
}

fn sync_with_remote(store: &Store, remote: &str) -> Result<()> {
    if !git::git_fetch(&store.root, remote)? {
        eprintln!("warning: fetch failed, continuing offline");
    }
    // Per SPEC §7.3 push order: state branch first, main second. If the
    // main push fails after the state push lands on the remote, the
    // next sync's half-push detector (below) surfaces the orphaned
    // state commit so the main push can be retried.
    if !store.stealth {
        sync_branch(&store.state_worktree_dir(), remote, "balls/tasks")?;
    }
    let main_branch = git::git_current_branch(&store.root)?;
    sync_branch(&store.root, remote, &main_branch)?;

    if !store.stealth {
        for id in detect_half_push(store)? {
            eprintln!(
                "warning: state branch records close for {id} but no `[{id}]` tag reachable from main"
            );
        }
    }
    Ok(())
}

/// Fetch + merge + push a single branch in `dir`. Retries once on push
/// failure to tolerate a contemporaneous remote advance.
fn sync_branch(dir: &Path, remote: &str, branch: &str) -> Result<()> {
    let remote_ref = format!("{remote}/{branch}");
    fetch_merge_resolve_at(dir, remote, &remote_ref)?;
    if git::git_push(dir, remote, branch).is_err() {
        fetch_merge_resolve_at(dir, remote, &remote_ref)?;
        git::git_push(dir, remote, branch)?;
    }
    Ok(())
}

/// Fetch from `remote` in `dir`, merge `remote_branch` into whatever HEAD
/// points at there, and auto-resolve task-file conflicts. Tolerates a
/// missing upstream branch (the "first push" case).
fn fetch_merge_resolve_at(dir: &Path, remote: &str, remote_branch: &str) -> Result<()> {
    let _ = git::git_fetch(dir, remote);
    // Remote branch may not exist yet (first push); ignore that error.
    if let Ok(git::MergeResult::Conflict) = git::git_merge(dir, remote_branch) {
        sync_resolve::auto_resolve_task_conflicts(dir)?;
        git::git_commit(dir, "state: auto-resolve sync conflicts")?;
    }
    Ok(())
}

pub fn cmd_resolve(file: String) -> Result<()> {
    let path = PathBuf::from(&file);
    let content = fs::read_to_string(&path)?;
    let (ours, theirs) = resolve::parse_conflict_markers(&content)?;
    let merged = resolve::resolve_conflict(&ours, &theirs);
    merged.save(&path)?;
    println!("resolved {file}");
    Ok(())
}

pub fn cmd_prime(identity: Option<String>, json: bool) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);

    // Try to sync; ignore failure
    let _ = cmd_sync("origin".to_string(), None);

    notify_claim_policy(&store);

    let tasks = store.all_tasks()?;
    let ready_tasks = ready::ready_queue(&tasks);
    let claimed: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.claimed_by.as_deref() == Some(&ident))
        .filter(|t| t.status == Status::InProgress)
        .collect();

    // `.ok()` collapses both no-git and ref-missing into None — prime_status
    // treats None as "skip the indicators" so we never special-case here.
    let main_branch = git::git_current_branch(&store.root).ok();
    let claimed_status: Vec<serde_json::Value> = claimed
        .iter()
        .map(|t| {
            let s = super::prime_status::for_task(&store, t, main_branch.as_deref());
            serde_json::json!({
                "id": t.id,
                "main_ahead": s.main_ahead,
                "overlap_files": s.overlap_files,
            })
        })
        .collect();

    if json {
        let obj = serde_json::json!({
            "identity": ident,
            "claimed": claimed,
            "ready": ready_tasks,
            "claimed_status": claimed_status,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
        return Ok(());
    }

    println!("=== balls prime: {ident} ===");
    for (t, status) in claimed.iter().zip(claimed_status.iter()) {
        let wt_dir = store
            .worktrees_root()
            .map(|r| r.join(&t.id))
            .unwrap_or_default();
        let main_ahead = status["main_ahead"].as_u64().unwrap_or(0);
        let overlap = status["overlap_files"].as_array().map_or(0, Vec::len);
        let suffix = match (main_ahead, overlap) {
            (0, _) => String::new(),
            (n, 0) => format!(" — main +{n} since claim"),
            (n, k) => format!(" — main +{n} since claim, {k} overlap"),
        };
        println!(
            "Claimed (resume): {} \"{}\" @ {}{}",
            t.id,
            t.title,
            wt_dir.display(),
            suffix,
        );
    }
    println!("Ready:");
    for t in &ready_tasks {
        println!("  [P{}] {} \"{}\"", t.priority, t.id, t.title);
    }
    println!("===");
    Ok(())
}

/// One-time hint when a new clone first sees a remote-set
/// `require_remote_on_claim`. Uses the resolved policy with no CLI
/// override (prime isn't a claim — it's just the bootstrap moment to
/// show the policy).
fn notify_claim_policy(store: &Store) {
    let Ok(cfg) = store.load_config() else { return };
    let local = policy::LocalConfig::load(store).ok().flatten();
    let resolved = policy::resolve(
        cfg.require_remote_on_claim,
        local.as_ref(),
        policy::SyncOverride::Unset,
    );
    policy::notify_repo_default_once(store, resolved);
}

pub fn cmd_repair(
    fix: bool,
    forget_half_push: Vec<String>,
    forget_all_half_pushes: bool,
) -> Result<()> {
    let store = discover()?;

    // Forget actions run first: they're surgical and only touch the
    // state branch. They don't depend on, or interact with, the
    // task-file scan or orphan-cleanup paths below.
    if !forget_half_push.is_empty() || forget_all_half_pushes {
        if store.no_git || store.stealth {
            return Err(BallError::Other(
                "--forget-half-push requires a non-stealth git-backed repo".into(),
            ));
        }
        let flagged = detect_half_push(&store)?;
        let targets: Vec<String> = if forget_all_half_pushes {
            flagged.clone()
        } else {
            for id in &forget_half_push {
                if !flagged.contains(id) {
                    return Err(BallError::Other(format!(
                        "{id} is not a currently-flagged half-push; nothing to forget"
                    )));
                }
            }
            forget_half_push.clone()
        };
        if targets.is_empty() {
            println!("No half-push warnings to forget.");
        } else {
            write_forget_half_push(&store, &targets)?;
            for id in &targets {
                println!("forgot half-push: {id}");
            }
        }
        return Ok(());
    }

    let dir = store.tasks_dir();
    let mut bad = Vec::new();
    if dir.exists() {
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Err(err) = Task::load(&p) {
                bad.push((p, err.to_string()));
            }
        }
    }
    if bad.is_empty() {
        println!("All task files OK.");
    } else {
        for (p, e) in &bad {
            println!("BAD: {} - {}", p.display(), e);
        }
    }
    if fix && !store.no_git {
        let (rc, rw) = worktree::cleanup_orphans(&store)?;
        for id in &rc {
            println!("removed orphan claim: {id}");
        }
        for id in &rw {
            println!("removed orphan worktree: {id}");
        }
    }
    Ok(())
}

