//! §9 base-change authoring — parse the verb's [`Flags`] into the [`BaseChange`]
//! it seals, with the per-verb [`guards`] run first and the flag→edge translation
//! delegated to [`build`]. Lifted from [`crate::mutate`] so the dispatch there
//! stays engine wiring; this is the verb→diff half.

use std::io;
use std::path::Path;

use crate::change::{Create, FieldEdit, Occupancy, Retire, Update};
use crate::id::IdScheme;
use crate::lifecycle::BaseChange;
use crate::task::Task;
use crate::taskfile::{read_task, task_ids};
use crate::verb::Verb;
use crate::wire::Command;

use super::{build, edit, guards, other, Flags};

/// A verb's authored change plus the ball's op-start state (the §7
/// `current_state` a `pre` plugin sees — `None` on `create`, which has no prior
/// ball).
pub(super) type Authored = (Box<dyn BaseChange>, Option<Task>);

/// Author the verb's [`BaseChange`] from the parsed `flags` (see [`Authored`]).
/// `now` is injected, so the change stays pure (it never reads a clock); the
/// `editor` seam serves only `update --edit`. `Ok(None)` is `--edit`'s
/// unchanged-buffer no-op — there is nothing to author. Only the five mutating
/// verbs reach here.
pub(super) fn base_change(
    verb: Verb,
    store: &Path,
    flags: &Flags,
    now: i64,
    editor: &mut edit::Editor,
) -> io::Result<Option<Authored>> {
    let actor = flags.actor.clone();
    match verb {
        Verb::Create => {
            guards::forbid_removals_on_create(flags)?;
            let title = one_positional(flags, "create")?;
            // `--subtask-of` folds into the parent + a claim-gate edge (§10).
            let parent = build::effective_parent(flags)?;
            let blockers = build::needs_blockers(flags)?;
            let blocks = build::blocks_edges(flags, parent.as_deref())?;
            build::require_live(
                store,
                verb,
                blockers.iter().map(|b| b.id.as_str()).chain(blocks.iter().map(|(id, _)| id.as_str())),
            )?;
            let base = Create {
                id: IdScheme::default().generate(),
                actor,
                now,
                title,
                parent: parent.clone(),
                priority: flags.priority,
                tags: flags.tags.clone(),
                blockers,
                blocks,
                body: flags.body.clone(),
                message: flags.message.clone(),
                existing: task_ids(store)?,
            };
            Ok(Some((Box::new(base), None)))
        }
        Verb::Claim | Verb::Unclaim => {
            guards::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let claimant = (verb == Verb::Claim).then(|| actor.clone());
            let base = Occupancy { verb, id, claimant, actor, now, message: flags.message.clone() };
            Ok(Some((Box::new(base), Some(before))))
        }
        Verb::Update => {
            guards::forbid_foreign_blocks(flags, verb)?;
            guards::forbid_contradictions(flags)?;
            let mut positionals = flags.positionals.iter();
            let id = positionals.next().ok_or_else(|| crate::usage("update: needs a task id"))?.clone();
            let before = read_task(store, &id)?;
            let edits = if flags.edit {
                // `--edit`: the buffer IS the payload — field flags and key=value
                // extras would race over it, so they are mutually exclusive (§9).
                guards::forbid_fields_with_edit(flags)?;
                if positionals.next().is_some() {
                    return Err(crate::usage("update: --edit and key=value extras are mutually exclusive — the buffer is the payload"));
                }
                let Some(after) = editor.edited(&before, &id)? else { return Ok(None) };
                vec![FieldEdit::Replace(Box::new(after))]
            } else {
                build::edits(positionals, flags)?
            };
            // Only the flag-minted edges are validated (§10, bl-6b8c): `--edit`'s
            // whole-buffer Replace is the blessed hand-stitch escape hatch, and a
            // RemoveBlocker unlink is the dangling-edge remedy — never refused.
            build::require_live(
                store,
                verb,
                edits.iter().filter_map(|e| match e {
                    FieldEdit::AddBlocker(b) => Some(b.id.as_str()),
                    _ => None,
                }),
            )?;
            let base = Update { id, actor, now, edits, message: flags.message.clone() };
            Ok(Some((Box::new(base), Some(before))))
        }
        Verb::Close => {
            guards::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let base = Retire { id, title: before.title.clone(), actor, message: flags.message.clone() };
            Ok(Some((Box::new(base), Some(before))))
        }
        // The diffless verbs never reach run()'s mutating branch; reject defensively.
        _ => Err(other(format!("{}: not a mutating verb", verb.token()))),
    }
}

/// The §7 `command` — the op plus its body intent. `body_change` is the new
/// markdown ball body (`--body`) when the op rewrites it (§7). Field-level
/// changes are NOT carried here (single source of truth, bl-3bfd §15): a plugin
/// reads them from the change worktree / the `before`/`after` states, not a
/// second diff description. Its presence (vs the diffless `None`) marks this a
/// ball-mutating op (§7).
pub(super) fn command(verb: Verb, flags: &Flags) -> Command {
    Command { op: verb.token().to_string(), body_change: flags.body.clone() }
}

/// The single positional `verb` expects (a `create` title, else a task id).
fn one_positional(flags: &Flags, verb: &str) -> io::Result<String> {
    match flags.positionals.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(crate::usage(format!("{verb}: expects exactly one positional argument"))),
    }
}
