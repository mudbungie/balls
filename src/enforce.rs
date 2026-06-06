//! §10 blocker enforcement — CORE, not a plugin.
//!
//! A blocker's MEANING is that it blocks ('Enforcement is CORE', §10): so the
//! claim/close guards live in core and are called from [`crate::change`] at
//! [`crate::lifecycle::BaseChange::stage`] — before the seal, so a refusal
//! aborts the op cleanly, and for `close` before any `close.pre` plugin (e.g.
//! delivery) squashes. There is no gating plugin: forge/build gates open gate
//! children and rely on those children BLOCKING, so the enforcer cannot be an
//! optional install.
//!
//! [`claim`] refuses a non-[`Task::ready`] ball (any unresolved CLAIM-blocker);
//! [`close`] refuses a non-[`Task::closeable`] one (any unresolved CLOSE-blocker,
//! i.e. gate). `drop` is never gated, so it has no guard. Resolution is
//! file-existence ([`crate::taskfile::exists`]): a blocker resolves when its
//! `tasks/<id>.md` is gone (closed/dropped, §10).

use std::io;
use std::path::Path;

use crate::task::{On, Task};
use crate::taskfile::exists;
use crate::verb::Verb;

/// Guard `claim`: `Ok` iff `task` is [`Task::ready`]. `id`/`dir` name the ball
/// and the change worktree the blockers resolve against.
pub(crate) fn claim(task: &Task, id: &str, dir: &Path) -> io::Result<()> {
    if task.ready(&|b| !exists(dir, b)) {
        Ok(())
    } else {
        Err(blocked(Verb::Claim, On::Claim, id, task, dir))
    }
}

/// Guard `close`: `Ok` iff `task` is [`Task::closeable`] — every CLOSE-blocker
/// (gate) resolved.
pub(crate) fn close(task: &Task, id: &str, dir: &Path) -> io::Result<()> {
    if task.closeable(&|b| !exists(dir, b)) {
        Ok(())
    } else {
        Err(blocked(Verb::Close, On::Close, id, task, dir))
    }
}

/// The refusal: a policy block (not corruption), naming the still-open blockers
/// gating `on` so the caller can see what to resolve.
fn blocked(verb: Verb, on: On, id: &str, task: &Task, dir: &Path) -> io::Error {
    let open: Vec<&str> = task
        .blockers
        .iter()
        .filter(|b| b.on == on && exists(dir, &b.id))
        .map(|b| b.id.as_str())
        .collect();
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{}: {id} blocked by unresolved {}", verb.token(), open.join(", ")),
    )
}

#[cfg(test)]
#[path = "enforce_tests.rs"]
mod tests;
