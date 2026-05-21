//! SPEC §9/§11 — command-level consumption of the push dispatch.
//!
//! `commands/lifecycle.rs` used to do `let _ = dispatch_push(...)`,
//! discarding a required participant's veto. This module is the
//! consumption seam: snapshot the rollback point before the event
//! commit, log the §11 overrides into that commit, then dispatch and
//! either roll the state branch back (required failure) or let the
//! best-effort `sync_status` the apply step folded in stand.
//!
//! Scope note (bl-fb4d, intentional per the ball's "WHY DEFERRED"):
//! rollback is of the *state branch* — the Task's durable source of
//! truth, which is exactly what SPEC §9's "rolled back to its
//! pre-event state" denotes. The review squash on `main` and the
//! claim worktree are owned by other commands' control flow; rewiring
//! those into one cross-command transaction is a separate blast
//! radius the SPEC staged out of this ball. `cmd_claim` opts out of
//! the generic path and rolls back via `drop_worktree` instead, which
//! is a clean un-claim.

use super::dispatch::{dispatch_push, DispatchInput, DispatchOutcome};
use crate::error::{BallError, Result};
use crate::git;
use crate::participant::Event;
use crate::participant_config::{override_log, InvocationOverrides};
use crate::store::Store;
use crate::task::Task;

/// State-branch HEAD sha. Captured *before* the command writes its
/// event commit it is the rewind target; captured *after*, it is the
/// `commit` the §5.1 side channel reports. `None` in stealth/no-git:
/// there is no state branch, so a required failure can only surface
/// as a non-zero exit (the metadata flip itself stands).
pub fn state_head(store: &Store) -> Result<Option<String>> {
    if store.stealth {
        return Ok(None);
    }
    Ok(Some(git::git_resolve_sha(
        &store.state_repo_dir(),
        "HEAD",
    )?))
}

/// SPEC §11 — fold the override audit log into the event's
/// state-branch commit (the one the command just made) by amending
/// its subject, so `git log --oneline` shows it next to the `[bl-id]`
/// tag. No-op on an empty fragment or in stealth, keeping default
/// invocations byte-identical (SPEC §12).
pub fn log_overrides(store: &Store, tokens: &[String]) -> Result<()> {
    let fragment = override_log(tokens);
    if fragment.is_empty() || store.stealth {
        return Ok(());
    }
    let dir = store.state_repo_dir();
    let out = git::clean_git_command(&dir)
        .args(["log", "-1", "--format=%B"])
        .output()?;
    let raw = String::from_utf8_lossy(&out.stdout);
    let new_msg = amended_message(raw.trim_end(), &fragment);
    let st = git::clean_git_command(&dir)
        .args(["commit", "--amend", "-m", &new_msg])
        .status()?;
    if !st.success() {
        return Err(BallError::Other(
            "failed to amend §11 override log into the state-branch commit".into(),
        ));
    }
    Ok(())
}

/// Pure subject-line rewrite: append `fragment` to the commit
/// subject, preserving the body. The fragment lands on the first
/// line so `git log --oneline` shows it next to the `[bl-id]` tag.
fn amended_message(msg: &str, fragment: &str) -> String {
    let (subject, body) = msg.split_once('\n').unwrap_or((msg, ""));
    if body.is_empty() {
        format!("{subject}{fragment}")
    } else {
        format!("{subject}{fragment}\n{body}")
    }
}

/// How a required failure rewinds the just-applied event.
pub enum Rollback<'a> {
    /// Hard-reset the state branch to this pre-event sha — undoes the
    /// create/update/review/close event commit so the Task is back to
    /// its pre-event state (SPEC §9). `None` in stealth/no-git.
    State(Option<&'a str>),
    /// `bl claim` only: un-claim via `drop_worktree` (removes the
    /// worktree, resets the task, commits the release) — a clean
    /// rollback the raw state-reset can't give because the worktree
    /// is a separate artifact.
    DropClaim,
}

/// Run the push dispatch for a command and consume its result. On a
/// required failure (Err — includes a first-class `reject`, §8.1)
/// apply `rollback` and return the error verbatim so `main` exits
/// non-zero with the reason. On success the best-effort `sync_status`
/// is already persisted by the apply step.
#[allow(clippy::too_many_arguments)]
pub fn finish(
    store: &Store,
    task_before: Option<&Task>,
    task: &Task,
    event: Event,
    identity: &str,
    overrides: &InvocationOverrides,
    tokens: &[String],
    rollback: Rollback<'_>,
) -> Result<DispatchOutcome> {
    log_overrides(store, tokens)?;
    let commit = state_head(store)?;
    let input = DispatchInput {
        store,
        task_before,
        task,
        event,
        identity,
        commit: commit.as_deref(),
        overrides,
        override_tokens: tokens,
    };
    match dispatch_push(&input) {
        Ok(outcome) => Ok(outcome),
        Err(e) => {
            match rollback {
                Rollback::State(Some(sha)) if !store.stealth => {
                    let _ = git::git_reset_hard(&store.state_repo_dir(), sha);
                }
                Rollback::State(_) => {}
                Rollback::DropClaim => {
                    let _ = crate::worktree::drop_worktree(store, &task.id, true);
                }
            }
            Err(e)
        }
    }
}

#[cfg(test)]
#[path = "consume_tests.rs"]
mod tests;
