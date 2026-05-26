//! `bl claim` — start work on a task: create its worktree (or a
//! `--no-worktree` metadata-only claim) and flip it to in_progress.
//! Split out of `lifecycle.rs` to keep that file under the 300-line
//! cap; re-exported from `commands` so callers stay byte-stable.

use super::plumbing::sync_inputs;
use super::{default_identity, discover};
use balls::error::{BallError, Result};
use balls::participant::Event;
use balls::participant_config::{override_tokens, InvocationOverrides};
use balls::plugin::{self, Rollback};
use balls::policy;
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
    let (cli, cfg, local) = sync_inputs(&store, sync, no_sync);
    let claim_policy = policy::resolve(
        cfg.as_ref().is_some_and(|c| c.require_remote_on_claim),
        local.as_ref(),
        cli,
    );
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
