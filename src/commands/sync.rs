//! sync, resolve, prime — remote reconciliation and agent bootstrap.

use super::half_push::detect_half_push;
use super::sync_report::apply_sync_report;
use super::sync_targets::push_recorded_targets;
use super::sync_review;
use super::{default_identity, discover};
use balls::error::Result;
use balls::store::Store;
use balls::task::{Status, Task};
use balls::{git, plugin, policy, ready, resolve, sanitize, sync_resolve};
use std::fs;
use std::path::{Path, PathBuf};

/// Arguments for `bl sync` and its staged-review variants. The struct
/// keeps the signature flat while letting `main.rs` thread CLI flags
/// through one bundle.
pub struct SyncArgs {
    pub remote: String,
    pub task: Option<String>,
    pub review: bool,
    pub apply: Option<String>,
    pub discard: Option<String>,
    pub list_staged: bool,
}

pub fn cmd_sync(args: SyncArgs) -> Result<()> {
    if let Some(id) = args.apply {
        return sync_review::apply_staged(&id);
    }
    if let Some(id) = args.discard {
        return sync_review::discard_staged(&id);
    }
    if args.list_staged {
        return sync_review::list_staged();
    }
    if args.review {
        return sync_review::stage_sync_event(&args.remote, args.task.as_deref());
    }
    cmd_sync_run(&args.remote, args.task.as_deref())
}

fn cmd_sync_run(remote: &str, task_filter: Option<&str>) -> Result<()> {
    let store = discover()?;
    if !store.no_git {
        sync_with_remote(&store, remote)?;
    }
    let ident = default_identity();
    match plugin::dispatch_sync(&store, task_filter, &ident) {
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
    // The main-branch and state-branch presence gates are independent
    // (bl-88c7) — pre-bl-88c7 a single code-remote gate fronted both,
    // so a hub-linked client with a reachable hub but no code `origin`
    // silently skipped the `balls/tasks` leg.
    let code_present = git::git_has_remote(&store.root, remote);

    if code_present && !git::git_fetch(&store.root, remote)? {
        eprintln!("warning: fetch failed, continuing offline");
    }

    // Per SPEC §7.3 push order: state branch first, main second. If the
    // main push fails after the state push lands on the remote, the
    // next sync's half-push detector (below) surfaces the orphaned
    // state commit so the main push can be retried.
    //
    // The state-leg presence gate runs against `state_worktree_dir()`,
    // not the project root (bl-16e9). In `master_url` mode the state
    // checkout is `.balls/state-repo/` — a balls-owned clone whose
    // `origin` is the hub — and the project root carries only the
    // user's code remotes (or none at all in the bridge-clone proxy
    // pattern). Asking the project root about the state remote would
    // silently skip the push in exactly the topology `master_url` was
    // built to enable. In legacy mode `state_worktree_dir()` is the
    // project's own `.balls/worktree`, a worktree that shares
    // `.git/config` with the project root, so the answer is identical
    // — the redirect is a strict superset of the old behavior.
    let mut state_synced = false;
    if !store.stealth {
        let state_remote = resolve_state_remote(store, remote);
        if git::git_has_remote(&store.state_worktree_dir(), &state_remote) {
            sync_branch(&store.state_worktree_dir(), &state_remote, "balls/tasks")?;
            state_synced = true;
        }
    }
    if code_present {
        let main_branch = store.load_config()?.integration_branch(&store.root)?;
        sync_branch(&store.root, remote, &main_branch)?;
        if !store.stealth {
            push_recorded_targets(store, remote, &main_branch)?;
        }
    }

    if !store.stealth && (code_present || state_synced) {
        for id in detect_half_push(store)? {
            eprintln!(
                "warning: state branch records close for {id} but no `[{id}]` tag reachable from main"
            );
        }
    }
    Ok(())
}

/// Resolve the remote the `balls/tasks` branch negotiates against.
/// Falls back to the code remote (`code_remote`) on an unset
/// `state_remote` *or* a config that won't load — sync stays resilient
/// to a corrupt `.balls/config.json` the same way the plugin leg does
/// (it warns, it doesn't abort), so a config a hub can't be read from
/// degrades to single-repo behavior rather than failing the command.
fn resolve_state_remote(store: &Store, code_remote: &str) -> String {
    let Ok(cfg) = store.load_config() else {
        return code_remote.to_string();
    };
    let local = policy::LocalConfig::load(store).ok().flatten();
    policy::state_remote_opt(&cfg, local.as_ref()).unwrap_or_else(|| code_remote.to_string())
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
    let _ = cmd_sync(SyncArgs {
        remote: "origin".to_string(),
        task: None,
        review: false,
        apply: None,
        discard: None,
        list_staged: false,
    });

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
    let main_branch = store.load_config().and_then(|c| c.integration_branch(&store.root)).ok();
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
            sanitize::inline(&t.title),
            wt_dir.display(),
            suffix,
        );
    }
    println!("Ready:");
    for t in &ready_tasks {
        println!("  [P{}] {} \"{}\"", t.priority, t.id, sanitize::inline(&t.title));
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
