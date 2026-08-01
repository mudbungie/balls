//! §9 result emission — what a sealed mutating op prints. The dispatch in
//! [`crate::mutate`] authors and seals the change; this is the one place that
//! turns the sealed commit into output.
//!
//! stdout is the SOLE machine channel: `create` prints the minted id ALONE so
//! `id=$(bl create …)` captures it clean; every other mutating verb
//! (`claim`/`unclaim`/`update`/`close`) prints a terse human confirmation
//! to stderr, leaving stdout empty.
//!
//! `create` ALONE reads its id back from the sealed commit's `bl-id` trailer
//! ([`message::parse`]) rather than from the pre-seal [`crate::id::IdScheme`]
//! mint — a `create/pre` plugin may reassign the id, and §5's `finalize`
//! re-derives it from the change worktree it actually committed, so only the
//! commit is authoritative there. Every other verb NAMES the ball it operates
//! on: the op holds that id before it seals, so re-deriving it would be a second
//! representation of one fact — bought with a subprocess round-trip on the far
//! side of the plugin chain, which is exactly where bl-dede turned a landed
//! close into exit 1.
//!
//! And nothing here may fail the op. Reporting runs AFTER the seal is durable,
//! so a read that cannot confirm what is already committed WARNS on stderr and
//! exits 0 (the §12 warn pattern); only a store that cannot produce the commit
//! at all — genuine corruption — errors.

use std::io;
use std::path::Path;

use crate::git;
use crate::message;
use crate::verb::Verb;

/// Emit the op's result after a successful seal (§9): the minted id to stdout for
/// `create`, a terse confirmation to stderr for every other mutating verb. `id`
/// is the op's own subject ball, authoritative for every verb but `create`. A
/// `close` that leaves live children adds the §10 notice — diagnostic, never
/// authority (the §12 warn pattern): any child alive at a successful close was,
/// by the close-blocker guard, not gating, and its `parent:` now dangles
/// (display-only, §3).
pub(super) fn emit(verb: Verb, store: &Path, id: &str, sha: &str) -> io::Result<()> {
    if verb == Verb::Create {
        return minted(store, sha);
    }
    eprintln!("{} {id}", verb.token());
    if verb == Verb::Close {
        if let Some(notice) = children_notice(id, open_children(store, id)?) {
            eprintln!("{notice}");
        }
    }
    Ok(())
}

/// Render the §10 surviving-children notice for a close that left `n` live
/// children — number-agreeing ("1 open child", bl-3ddb), `None` when none
/// survive. Pure, so the wording is unit-testable.
pub(super) fn children_notice(id: &str, n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("notice: {id} closed with 1 open child, not gating — its parent pointer now dangles (display-only)")),
        n => Some(format!("notice: {id} closed with {n} open children, none gating — their parent pointers now dangle (display-only)")),
    }
}

/// How many live balls name `id` as their `parent` — the same containment scan
/// the `show` tree renders, reduced to a count.
fn open_children(store: &Path, id: &str) -> io::Result<usize> {
    let mut n = 0;
    for child in crate::taskfile::task_ids(store)? {
        if crate::taskfile::read_task(store, &child)?.parent.as_deref() == Some(id) {
            n += 1;
        }
    }
    Ok(n)
}

/// Print `create`'s minted id: the `bl-id` trailer of the sealed commit `sha` on
/// the STORE, read back from what was actually committed (§5) so a `create/pre`
/// plugin's reassignment is what the caller captures.
///
/// The read is best-effort BY CONSTRUCTION. The ball exists the moment `sha` is
/// on the store branch, so a trailer this cannot recover is a lost id, not a
/// lost ball: it warns and exits 0, naming where the answer is. The `.expect`
/// that used to stand here read the same situation as an impossible state and
/// panicked — telling `id=$(bl create …)`'s caller their ball was never created,
/// the expensive direction to lie in (bl-dede). Only `git log` itself failing —
/// no such commit, a store that cannot answer for its own tip — stays an error.
fn minted(store: &Path, sha: &str) -> io::Result<()> {
    let commit = git::run(store, &["log", "-1", "--format=%B", sha], None)?;
    let id = message::parse(&commit)
        .ok()
        .and_then(|mut md| md.remove("bl-id"))
        .and_then(|v| v.into_iter().next());
    match id {
        Some(id) => println!("{id}"),
        None => eprintln!("warning: sealed {sha} but could not confirm its id from the store — see `git log` on the tasks branch"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "mutate_report_tests.rs"]
mod tests;
