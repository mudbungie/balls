//! claim, review, close, drop — commands that mutate a task's own
//! lifecycle. `update` (field edits + the reject path) lives in
//! `update.rs`; dep/link graph ops in `dep_link.rs`.

use super::{default_identity, discover};
use balls::error::{BallError, Result};
use balls::participant::Event;
use balls::participant_config::{override_tokens, InvocationOverrides};
use balls::plugin::{self, Rollback};
use balls::policy::{self, ClaimPolicy, LocalConfig, SyncOverride};
use balls::store::Store;
use balls::worktree;

pub fn cmd_claim(
    id: String,
    identity: Option<String>,
    no_worktree: bool,
    sync: bool,
    no_sync: bool,
    overrides: InvocationOverrides,
) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    if store.no_git && !no_worktree {
        return Err(BallError::Other(
            "no git repo: use `bl claim --no-worktree` to claim without a worktree".into(),
        ));
    }
    let claim_policy = resolve_claim_policy(&store, sync, no_sync)?;
    if no_worktree {
        worktree::claim_no_worktree(&store, &id, &ident, claim_policy)?;
        println!("claimed {id} (no worktree)");
    } else {
        // Pre-image: the open task before the claim mutated it — the
        // diff basis a claim-mirror plugin sees (SPEC §5.1).
        let task_before = store.load_task(&id).ok();
        let path = worktree::create_worktree(&store, &id, &ident, claim_policy)?;
        let task = store.load_task(&id)?;
        let tokens = override_tokens(&overrides, sync, no_sync);
        // A required plugin veto un-claims (drop_worktree); best-
        // effort skips are recorded in sync_status (SPEC §9).
        plugin::finish(
            &store,
            task_before.as_ref(),
            &task,
            Event::Claim,
            &ident,
            &overrides,
            &tokens,
            Rollback::DropClaim,
        )?;
        let main_branch = store
            .load_config()?
            .integration_branch_for(&store.root, task.target_branch.as_deref())?;
        let _ = balls::git::git_merge(&path, &main_branch);
        println!("{}", path.display());
    }
    Ok(())
}

fn resolve_claim_policy(store: &Store, sync: bool, no_sync: bool) -> Result<ClaimPolicy> {
    let cli = match (sync, no_sync) {
        (true, false) => SyncOverride::Sync,
        (false, true) => SyncOverride::NoSync,
        _ => SyncOverride::Unset,
    };
    let repo_default = store
        .load_config()
        .map(|c| c.require_remote_on_claim)
        .unwrap_or(false);
    let local = LocalConfig::load(store)?;
    Ok(policy::resolve(repo_default, local.as_ref(), cli))
}

pub fn cmd_review(
    id: String,
    message: Vec<String>,
    identity: Option<String>,
    sync: bool,
    no_sync: bool,
    overrides: InvocationOverrides,
) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    let message = balls::commit_msg::join_messages(&message);
    let task_before = store.load_task(&id).ok();
    let rb = plugin::state_head(&store)?;
    // A `--no-worktree` claim leaves `task.branch` unset and never
    // creates `.balls-worktrees/<id>`. Such a task has no work branch to
    // squash, so it takes the same metadata-only flip as no-git mode —
    // routing it through `review_worktree` would spawn git in a worktree
    // dir that doesn't exist and fail with ENOENT (bl-7152).
    if store.no_git || store.load_task(&id)?.branch.is_none() {
        balls::review::review_no_git(&store, &id, message.as_deref(), &ident)?;
    } else {
        let (cli, cfg, local) = sync_inputs(&store, sync, no_sync)?;
        let repo = cfg.as_ref().is_some_and(|c| c.require_remote_on_review);
        let policy = policy::resolve_review(repo, local.as_ref(), cli);
        balls::review::review_worktree(&store, &id, message.as_deref(), &ident, policy)?;
    }
    let task = store.load_task(&id)?;
    let tokens = override_tokens(&overrides, sync, no_sync);
    plugin::finish(
        &store,
        task_before.as_ref(),
        &task,
        Event::Review,
        &ident,
        &overrides,
        &tokens,
        Rollback::State(rb.as_deref()),
    )?;
    let deferred = matches!(
        store.load_config()?.delivery_mode(),
        balls::config::DeliveryMode::Deferred
    );
    if deferred {
        println!("reviewed {id} (deferred) — gated until the forge PR merges");
    } else {
        println!("reviewed {id} — from the repo root, run `bl close {id} -m \"...\"` to finish");
    }
    Ok(())
}

pub fn cmd_close(
    id: String,
    message: Vec<String>,
    identity: Option<String>,
    sync: bool,
    no_sync: bool,
    overrides: InvocationOverrides,
) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    let message = balls::commit_msg::join_messages(&message);
    let task_before = store.load_task(&id).ok();
    let rb = plugin::state_head(&store)?;
    let task = if store.no_git {
        balls::review::close_no_git(&store, &id, message.as_deref(), &ident)?
    } else {
        let (cli, cfg, local) = sync_inputs(&store, sync, no_sync)?;
        let repo = cfg.as_ref().is_some_and(|c| c.require_remote_on_close);
        let policy = policy::resolve_close(repo, local.as_ref(), cli);
        balls::review::close_worktree(&store, &id, message.as_deref(), &ident, policy)?
    };
    let tokens = override_tokens(&overrides, sync, no_sync);
    // A required plugin veto rolls the state branch back to `rb`,
    // un-archiving the task (SPEC §9). The main squash / worktree
    // teardown stay owned by close_worktree, deferred per the SPEC.
    plugin::finish(
        &store,
        task_before.as_ref(),
        &task,
        Event::Close,
        &ident,
        &overrides,
        &tokens,
        Rollback::State(rb.as_deref()),
    )?;
    println!("closed {id}");
    if !store.no_git {
        println!("{}", store.root.display());
    }
    Ok(())
}

fn sync_inputs(
    store: &Store,
    sync: bool,
    no_sync: bool,
) -> Result<(SyncOverride, Option<balls::config::Config>, Option<LocalConfig>)> {
    let cli = match (sync, no_sync) {
        (true, false) => SyncOverride::Sync,
        (false, true) => SyncOverride::NoSync,
        _ => SyncOverride::Unset,
    };
    let cfg = store.load_config().ok();
    let local = LocalConfig::load(store)?;
    Ok((cli, cfg, local))
}

pub fn cmd_drop(id: String, force: bool) -> Result<()> {
    let store = discover()?;
    if store.no_git {
        worktree::drop_no_worktree(&store, &id)?;
    } else {
        worktree::drop_worktree(&store, &id, force)?;
    }
    // SPEC §6.2: observe-only notification. Best-effort, never blocks
    // or fails the drop. The post-drop task (claim released, back to
    // open) is what a subscribed native plugin mirrors as a
    // walk-away. Legacy plugins never subscribe `drop`, so this is a
    // no-op for them — `bl drop` stays byte-identical.
    if let Ok(task) = store.load_task(&id) {
        let _ = plugin::dispatch_drop(&store, &task, &default_identity());
    }
    println!("dropped {id}");
    Ok(())
}
