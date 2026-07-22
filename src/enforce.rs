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

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use crate::task::{Blocker, Task};
use crate::taskfile::{exists, read_task};
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

/// Is `on` a rung of the resolution lifecycle? A ball resolves by CLOSING
/// (usually via claim — the work has to happen somewhere), so only an edge
/// gating `claim` or `close` can keep a ball from ever resolving; an edge on
/// any other op leaves close reachable and can never strand a loop.
fn lifecycle(on: Verb) -> bool {
    matches!(on, Verb::Claim | Verb::Close)
}

/// DFS from `from` toward `target` over lifecycle edges: the hop list
/// `[(on, id), …]` of a waits-on path when one exists, `None` when `target` is
/// unreachable. Resolution is file-existence (§10), so an unreadable ball is a
/// resolved one — no live edges out; `seen` keeps a pre-existing loop off the
/// new edge's path from walking forever.
fn waits(dir: &Path, from: &str, target: &str, seen: &mut BTreeSet<String>) -> Option<Vec<(Verb, String)>> {
    if !seen.insert(from.to_string()) {
        return None;
    }
    let Ok(task) = read_task(dir, from) else {
        return None;
    };
    for b in task.blockers.iter().filter(|b| lifecycle(b.on)) {
        if b.id == target {
            return Some(vec![(b.on, b.id.clone())]);
        }
        if let Some(mut hops) = waits(dir, &b.id, target, seen) {
            hops.insert(0, (b.on, b.id.clone()));
            return Some(hops);
        }
    }
    None
}

/// §10 write-time acyclicity (bl-54fe): refuse a front-door blocker edge
/// (`--needs`/`--blocks`/`--subtask-of`) that closes a claim/close cycle —
/// `blocked` waits on `edge`, and `edge.id` already waits back on `blocked`.
/// No claim→close order resolves such a loop, `bl list` renders it healthy (a
/// close-blocker never shows), and the refusal otherwise lands at `bl close`
/// with the work already done. Called at stage AFTER the op's writes, so the
/// staged tree holds the union of old and new edges; only edges THIS op spelled
/// are checked — a pre-existing cycle never refuses an unrelated edit, the
/// in-band unlink (`--no-needs`) always passes, and `update --edit`/`import`
/// stay the verbatim hand-stitch escape hatches (§10).
pub(crate) fn acyclic(dir: &Path, verb: Verb, blocked: &str, edge: &Blocker) -> io::Result<()> {
    if !lifecycle(edge.on) {
        return Ok(());
    }
    let hops = if edge.id == blocked {
        Some(Vec::new()) // self-edge: the loop is the edge itself
    } else {
        waits(dir, &edge.id, blocked, &mut BTreeSet::new())
    };
    let Some(hops) = hops else {
        return Ok(());
    };
    let cycle: String = std::iter::once(format!("{blocked} -{}-> {}", edge.on.token(), edge.id))
        .chain(hops.iter().map(|(on, id)| format!(" -{}-> {id}", on.token())))
        .collect();
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "{}: blocker '{}' on {blocked} closes a deadlock: {cycle} — no claim/close order resolves that loop, and it only surfaces at close, after the work (bl-54fe). A verification gate is ONE edge, either direction but not both: --needs <parent> alone (verify after the parent delivers), or the close-gate alone (--subtask-of <parent>: verify INSIDE the parent's branch, see `bl create --skill`). Unlink an edge with `bl update <id> --no-needs <id>`",
            verb.token(),
            edge.id
        ),
    ))
}

#[cfg(test)]
#[path = "enforce_tests.rs"]
mod tests;
