//! `bl claim` — start work on a task: create its worktree (or a
//! `--no-worktree` metadata-only claim) and flip it to in_progress.
//! Split out of `lifecycle.rs` to keep that file under the 300-line
//! cap; re-exported from `commands` so callers stay byte-stable.

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
        .is_ok_and(|c| c.require_remote_on_claim);
    let local = LocalConfig::load(store)?;
    Ok(policy::resolve(repo_default, local.as_ref(), cli))
}
