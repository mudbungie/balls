//! §9 result emission — what a sealed mutating op prints. The dispatch in
//! [`crate::mutate`] authors and seals the change; this is the one place that
//! turns the sealed commit into output.
//!
//! stdout is the SOLE machine channel: `create` prints the minted id ALONE so
//! `id=$(bl create …)` captures it clean; every other mutating verb
//! (`claim`/`unclaim`/`update`/`close`) prints a terse human confirmation
//! to stderr, leaving stdout empty. The id is read back from the sealed commit's
//! `bl-id` trailer ([`message::parse`]), NOT the pre-seal [`crate::id::IdScheme`]
//! mint — so a `create/pre` plugin's id reassignment is reflected correctly (§5:
//! `finalize` re-derives the id from the change worktree it actually committed).

use std::io;
use std::path::Path;

use crate::git;
use crate::message;
use crate::verb::Verb;

/// Emit the op's result after a successful seal (§9): the minted id to stdout for
/// `create`, a terse confirmation to stderr for every other mutating verb. A
/// `close` that leaves live children adds the §10 notice — diagnostic, never
/// authority (the §12 warn pattern): any child alive at a successful close was,
/// by the close-blocker guard, not gating, and its `parent:` now dangles
/// (display-only, §3).
pub(super) fn emit(verb: Verb, store: &Path, sha: &str) -> io::Result<()> {
    let id = sealed_id(store, sha)?;
    if verb == Verb::Create {
        println!("{id}");
    } else {
        eprintln!("{} {id}", verb.token());
    }
    if verb == Verb::Close {
        if let Some(notice) = children_notice(&id, open_children(store, &id)?) {
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

/// The `bl-id` trailer of the sealed commit `sha` on the STORE — the op's task id
/// read back from what was actually committed (§5). Every mutating verb seals a
/// `bl-id` (core authors it at finalize; plugins have no return channel), so its
/// absence is an impossible-state invariant.
fn sealed_id(store: &Path, sha: &str) -> io::Result<String> {
    let commit = git::run(store, &["log", "-1", "--format=%B", sha], None)?;
    Ok(message::parse(&commit)?
        .remove("bl-id")
        .and_then(|v| v.into_iter().next())
        .expect("a mutating op always seals a bl-id trailer (§5)"))
}
