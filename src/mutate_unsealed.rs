//! The DELIVERED-BUT-UNSEALED close (bl-739b) — re-voicing a `close` that
//! aborted AFTER its delivery had already landed.
//!
//! `close` is two acts and they are not atomic against a concurrent `bl`. The
//! delivery squash lands on the target ref in `close.pre` — the BINDING commit
//! point, whose §14 rollback DECLINES on purpose — and only then does core seal
//! the task file onto the store. When that seal loses its §8 compare-and-swap
//! the op aborts with "the store moved under this op … nothing was written",
//! which is true of the STORE and false of the operator's code: the squash is
//! on `main`, the ball still reads claimed, the worktree is still up, and the
//! whole state reads exactly like a close that never landed. Three agents in
//! one session read it that way and were about to redo delivered work.
//!
//! The answer is VOICE, not retry. §14 converge-on-retry is the rule and the
//! retry is one command (docs/architecture.md §8; bl-fa89, re-affirmed for this
//! exact question in docs/design/bl-cdec-atomicity.md), so core still does not
//! loop. What changes is that it stops overstating its scope. The seal refusal
//! ([`crate::git`]) is generic — every mutating verb loses that CAS the same
//! way — so it cannot say this: on a `create` that lost, "your code is already
//! on main" would be a lie. The close-specific half therefore lives HERE, in
//! the one dispatch that knows the verb, and it is DERIVED rather than assumed:
//! it asks the project repo where the ball's delivery commit actually is
//! ([`crate::delivery_repo::Project::delivered_since_fork`], the same
//! fork-scoped tag scan the retry converges on) and stays silent when nothing
//! stands. A failed delivery gate, a stale source, a rejected delivery CAS all
//! abort BEFORE the squash and get no note; only a genuinely
//! delivered-but-unsealed close does.

use std::io;
use std::path::Path;

use crate::delivery::Repo;
use crate::delivery_path::{marker, work_branch};
use crate::delivery_repo::Project;
use crate::verb::Verb;

/// Amend a mutating op's abort with the delivered-but-unsealed note, iff the op
/// was a `close` whose delivery already stands in the project repo at `root`
/// (the §7 invocation path). Every other verb — and every close that aborted
/// before its squash — gets its error back untouched, kind included, so the
/// usage/operational taxonomy [`crate::dispatch`] reads is preserved.
pub(super) fn amend(err: io::Error, root: &Path, verb: Verb, id: &str, target: Option<&str>) -> io::Error {
    if verb != Verb::Close {
        return err;
    }
    let Some((sha, branch)) = landed(&Project::at(root), id, target) else {
        return err;
    };
    io::Error::new(err.kind(), format!("{err}\n\n{}", note(id, &sha, &branch)))
}

/// Where this ball's delivery commit stands, if it stands at all: the squash's
/// sha and the ref carrying it. The target ref is READ, never minted — a nested
/// ball's `work/<target>` (bl-7b71), else the project's integration branch — so
/// a diagnostic can create nothing. Every git failure is SILENCE: no project
/// repo, a `work/<id>` branch never made, a target ref that does not exist.
/// This runs on a path that has already failed, and it withholds what it cannot
/// prove rather than replacing one abort with another.
fn landed(project: &Project, id: &str, target: Option<&str>) -> Option<(String, String)> {
    let branch = match target {
        Some(target) => work_branch(target),
        None => project.integration().ok()?,
    };
    let sha = project.delivered_since_fork(&work_branch(id), &branch, &marker(id)).ok()??;
    Some((sha, branch))
}

/// The note — pure, so its wording is unit-testable, and phrased to correct the
/// abort it follows rather than to repeat it: it names the commit and the ref
/// (the neighbouring delivery refusals name S, T and P the same way), says
/// which half is actually outstanding, and prescribes the one command that
/// finishes the op.
fn note(id: &str, sha: &str, branch: &str) -> String {
    format!(
        "delivered, not sealed: this close ALREADY landed its code — the {} delivery commit {sha} \
         is on {branch}. What the abort above did not write is the TASK FILE, which is why {id} \
         still reads claimed and its work worktree is still up. Do not redo the work and do not \
         unclaim. Re-run `bl close`: it converges on the standing delivery (no second squash) and \
         seals the task file onto the store's current tip",
        marker(id)
    )
}

#[cfg(test)]
#[path = "mutate_unsealed_tests.rs"]
mod tests;
