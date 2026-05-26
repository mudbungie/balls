//! review, close, drop — commands that mutate a task's own lifecycle.
//! `claim` lives in `claim.rs`, `update` (field edits + the reject
//! path) in `update.rs`; dep/link graph ops in `dep_link.rs`.

use super::plumbing::{finish_state_event, sync_inputs};
use super::{default_identity, discover};
use balls::error::Result;
use balls::participant::Event;
use balls::participant_config::InvocationOverrides;
use balls::plugin;
use balls::policy;
use balls::worktree;

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
    finish_state_event(&store, Event::Review, &ident, &overrides, sync, no_sync, || {
        // A `--no-worktree` claim leaves `task.branch` unset and never
        // creates `.balls-worktrees/<id>`. Such a task has no work
        // branch to squash, so it takes the same metadata-only flip as
        // no-git mode — routing it through `review_worktree` would
        // spawn git in a worktree dir that doesn't exist and fail with
        // ENOENT (bl-7152).
        if store.no_git || store.load_task(&id)?.branch.is_none() {
            balls::review::review_no_git(&store, &id, message.as_deref(), &ident)?;
        } else {
            let (cli, cfg, local) = sync_inputs(&store, sync, no_sync);
            let repo = cfg.as_ref().is_some_and(|c| c.require_remote_on_review);
            let policy = policy::resolve_review(repo, local.as_ref(), cli);
            balls::review::review_worktree(&store, &id, message.as_deref(), &ident, policy)?;
        }
        let task = store.load_task(&id)?;
        Ok((task_before, task))
    })?;
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

#[allow(clippy::too_many_arguments)]
pub fn cmd_close(
    id: String,
    message: Vec<String>,
    identity: Option<String>,
    delivered: Option<String>,
    delivered_repo: Option<String>,
    resolve_remote: bool,
    sync: bool,
    no_sync: bool,
    overrides: InvocationOverrides,
) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);
    let message = balls::commit_msg::join_messages(&message);
    let task_before = store.load_task(&id).ok();
    // A required plugin veto rolls the state branch back, un-archiving
    // the task (SPEC §9, owned by `finish_state_event`). The main
    // squash / worktree teardown stay owned by `close_worktree`,
    // deferred per the SPEC.
    finish_state_event(&store, Event::Close, &ident, &overrides, sync, no_sync, || {
        let task = if store.no_git {
            balls::review::close_no_git(&store, &id, message.as_deref(), &ident)?
        } else {
            let (cli, cfg, local) = sync_inputs(&store, sync, no_sync);
            let repo = cfg.as_ref().is_some_and(|c| c.require_remote_on_close);
            let policy = policy::resolve_close(repo, local.as_ref(), cli);
            // bl-e454: deferred mode is exactly the case where the
            // closer is typically not the clone that produced the
            // squash (the forge merged, the bridge's sync hook is
            // closing), so the cross-repo fallback auto-engages
            // without a flag. Local-squash mode keeps the
            // explicit-opt-in default — single-repo closes stay
            // byte-identical.
            let deferred = matches!(
                cfg.as_ref().map(balls::config::Config::delivery_mode),
                Some(balls::config::DeliveryMode::Deferred)
            );
            balls::review::close_worktree(
                &store,
                &id,
                message.as_deref(),
                &ident,
                policy,
                delivered,
                delivered_repo,
                resolve_remote || deferred,
            )?
        };
        Ok((task_before, task))
    })?;
    println!("closed {id}");
    if !store.no_git {
        println!("{}", store.root.display());
    }
    Ok(())
}

pub fn cmd_drop(id: String, force: bool) -> Result<()> {
    let store = discover()?;
    // Validate the project config before the claim is released: a
    // structurally invalid `project.json` (a `drop` subscription that
    // violates observe-only, SPEC §6.2) is a precondition error, not
    // something the observe-only dispatch below can surface — it
    // swallows failures by design.
    store.load_project_config()?;
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
