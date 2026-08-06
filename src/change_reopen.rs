//! `reopen` (§9) — the base change that writes a dead ball's `tasks/<id>.md`
//! back. Lifted to a sibling module so [`super`] stays the verb table.
//!
//! **The content is INJECTED, not read here.** Every [`BaseChange`] impl is
//! git-free by construction (it reads and writes the change worktree directly,
//! leaving the [`crate::lifecycle::Engine`]'s anvil the only git in an op), and
//! the ball being restored exists only in `balls/tasks` HISTORY. So the
//! reconstruction — [`crate::reads::resolve_dead`], the same recency walk `bl
//! show <dead-id>` and `bl list -s closed` already share — happens at AUTHORING
//! time next to the clock and the minted id, and hands the finished [`Task`]
//! here. That also puts reopen's two refusals where they can be spelled well
//! (a live id, an id that names nothing) instead of after a worktree and a
//! plugin chain have been paid for.
//!
//! What is left is the §10 gate every mutating op runs — reopen is not exempt,
//! because a blocker names ANY op and `on = "reopen"` is expressible the moment
//! the verb is (§10/§15). No carve-out, no special case.

use std::io;
use std::path::Path;

use crate::enforce;
use crate::lifecycle::BaseChange;
use crate::task::Task;
use crate::taskfile::write_task;
use crate::verb::Verb;

use super::finalize_titled;

/// `reopen` (§9): restore a retired ball by writing back the `task` reconstructed
/// from the newest deletion's parent tree.
///
/// `updated` is restamped like every other op — the restore IS a transition, and
/// the op instant is the one clock read (§8). Everything else lands verbatim:
/// `created` is the ball's birth and rewriting it would destroy the record, and
/// the blockers/tags/parent it died with are still the operator's declarations.
/// The one field a close can falsify — `claimant`, which named a worktree that
/// close then tore down — is dropped by `--clean` at authoring, never implicitly
/// (an unforced automation is friction, and the operator says which they want).
pub struct Reopen {
    pub id: String,
    /// The ball as it stood the instant before its newest deletion (already
    /// `--clean`-filtered if the operator asked), injected — see the module note.
    pub task: Task,
    pub actor: String,
    pub now: i64,
    /// The `-m` free commit-message narration (§5); reopen edits no ball field.
    pub message: Option<String>,
}

impl BaseChange for Reopen {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        enforce::gate(&self.task, Verb::Reopen, &self.id, dir)?;
        let mut task = self.task.clone();
        task.updated = self.now;
        write_task(dir, &self.id, &task)
    }

    fn finalize(&self, dir: &Path) -> io::Result<String> {
        finalize_titled(dir, Verb::Reopen, &self.actor, &self.id, self.message.as_deref())
    }
}

#[cfg(test)]
#[path = "change_reopen_tests.rs"]
mod tests;
