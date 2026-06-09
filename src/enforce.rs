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
//! The whole model is ONE op-keyed guard ([`gate`], §10/§15): for op `O`, refuse
//! while any blocker that NAMES `O` (`on == O`) is unresolved. `on` is ANY op, so
//! every mutating verb routes through it — a blocker on a third op is enforced by
//! core, no per-op carve-out. [`claim`]/[`close`] are its two named, sugar-bearing
//! spellings (`claim` over [`Task::ready`], `close` over [`Task::closeable`]); the
//! rest (`update`/`unclaim`) call [`gate`] directly. `create` has no prior
//! ball, so nothing gates it. Resolution is file-existence
//! ([`crate::taskfile::exists`]): a blocker resolves when its `tasks/<id>.md` is
//! gone (closed/dropped, §10).

use std::io;
use std::path::Path;

use crate::task::Task;
use crate::taskfile::exists;
use crate::verb::Verb;

/// Guard `claim`: `Ok` iff `task` is [`Task::ready`] — the `on == claim` case of
/// [`gate`], named for the readiness predicate it spells (§10). `id`/`dir` name
/// the ball and the change worktree the blockers resolve against.
pub(crate) fn claim(task: &Task, id: &str, dir: &Path) -> io::Result<()> {
    if task.ready(&|b| !exists(dir, b)) {
        Ok(())
    } else {
        Err(blocked(Verb::Claim, id, task, dir))
    }
}

/// Guard `close`: `Ok` iff `task` is [`Task::closeable`] — the `on == close`
/// case of [`gate`], named for the gate predicate it spells (§10).
pub(crate) fn close(task: &Task, id: &str, dir: &Path) -> io::Result<()> {
    if task.closeable(&|b| !exists(dir, b)) {
        Ok(())
    } else {
        Err(blocked(Verb::Close, id, task, dir))
    }
}

/// The generic op-keyed guard (§10/§15): refuse op `verb` while any blocker with
/// `on == verb` is unresolved. claim/close have named spellings above; every
/// other mutating op calls this, so a blocker on ANY op is enforced.
pub(crate) fn gate(task: &Task, verb: Verb, id: &str, dir: &Path) -> io::Result<()> {
    if task.blockers.iter().any(|b| b.on == verb && exists(dir, &b.id)) {
        Err(blocked(verb, id, task, dir))
    } else {
        Ok(())
    }
}

/// The refusal: a policy block (not corruption), naming the still-open blockers
/// that gate `verb` (`on == verb`) so the caller can see what to resolve.
fn blocked(verb: Verb, id: &str, task: &Task, dir: &Path) -> io::Error {
    let open: Vec<&str> = task
        .blockers
        .iter()
        .filter(|b| b.on == verb && exists(dir, &b.id))
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
