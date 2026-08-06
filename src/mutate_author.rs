//! §9 base-change authoring — parse the verb's [`Flags`] into the [`BaseChange`]
//! it seals, with the per-verb [`guards`] run first and the flag→edge translation
//! delegated to [`build`]. Lifted from [`crate::mutate`] so the dispatch there
//! stays engine wiring; this is the verb→diff half.

use std::io;
use std::path::Path;

use crate::change::{Create, FieldEdit, Occupancy, Reopen, Retire, Update};
use crate::id::IdScheme;
use crate::lifecycle::BaseChange;
use crate::task::Task;
use crate::taskfile::{exists, read_task, task_ids};
use crate::verb::Verb;
use crate::wire::Command;

use super::{build, edit, guards, other, Flags};

/// A verb's authored change plus the ball's op-start state (the §7
/// `current_state` a `pre` plugin sees — `None` on `create`, which has no prior
/// ball), plus the BALL ID the op is about.
pub(super) struct Authored {
    pub base: Box<dyn BaseChange>,
    pub before: Option<Task>,
    /// The op's ball: the verb's positional, or `create`'s freshly minted id.
    /// Authored HERE because this is where identity enters the op, and carried
    /// onto the §7 wire ([`crate::wire::Command::id`]) so no plugin re-derives
    /// it from the change worktree (§0 obligation 4; bl-a5f3).
    pub id: String,
}

/// Author the verb's [`BaseChange`] from the parsed `flags` (see [`Authored`]).
/// `now` and `roots` (this checkout's [`crate::delivery_repo::Project::root_commits`])
/// are injected, so the change stays pure (it reads no clock and shells no git):
/// `create` STAMPS the first (canonical) root on the ball, `claim` ADMITS a ball
/// recorded against ANY of them (bl-0161), the other verbs ignore it. The
/// `editor` seam serves only `update --edit`. `Ok(None)` is `--edit`'s
/// unchanged-buffer no-op — there is nothing to author. Only the five mutating
/// verbs reach here.
pub(super) fn base_change(
    verb: Verb,
    store: &Path,
    flags: &Flags,
    now: i64,
    roots: Vec<String>,
    editor: &mut edit::Editor,
) -> io::Result<Option<Authored>> {
    let actor = flags.actor.clone();
    guards::forbid_clean_outside_reopen(flags, verb)?;
    match verb {
        Verb::Create => {
            guards::forbid_removals_on_create(flags)?;
            let title = one_positional(flags, "create")?;
            // `--subtask-of` folds into the parent + a close-gate edge (§10) —
            // together the §11 nesting declaration (`crate::target`).
            let parent = build::effective_parent(flags)?;
            let blockers = build::needs_blockers(flags)?;
            let blocks = build::blocks_edges(flags, parent.as_deref())?;
            build::require_live(
                store,
                verb,
                blockers.iter().map(|b| b.id.as_str()).chain(blocks.iter().map(|(id, _)| id.as_str())),
            )?;
            // The § id-generation collision rule, core's half: the draw is
            // re-rolled off the LIVE set (bl-1fc4) rather than written blind —
            // a blind draw landing on a live id staged over that ball and died
            // at finalize as a phantom "a create.pre plugin reassigned…" abort,
            // with no plugin in the chain. Only a plugin's explicit
            // reassignment still aborts there.
            let existing = task_ids(store)?;
            let id = IdScheme::default().mint(&existing)?;
            let base = Create {
                id: id.clone(),
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
                root_commit: roots.into_iter().next(),
                existing,
            };
            Ok(Some(Authored { base: Box::new(base), before: None, id }))
        }
        Verb::Claim | Verb::Unclaim => {
            guards::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let claimant = (verb == Verb::Claim).then(|| actor.clone());
            let base = Occupancy {
                verb,
                id: id.clone(),
                claimant,
                actor,
                now,
                message: flags.message.clone(),
                current_roots: roots,
            };
            Ok(Some(Authored { base: Box::new(base), before: Some(before), id }))
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
            let base = Update { id: id.clone(), actor, now, edits, message: flags.message.clone() };
            Ok(Some(Authored { base: Box::new(base), before: Some(before), id }))
        }
        Verb::Close => {
            guards::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let base =
                Retire { id: id.clone(), title: before.title.clone(), actor, message: flags.message.clone() };
            Ok(Some(Authored { base: Box::new(base), before: Some(before), id }))
        }
        Verb::Reopen => {
            guards::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let task = restored(store, &id, flags.clean)?;
            let base = Reopen { id: id.clone(), task, actor, now, message: flags.message.clone() };
            // `before` is None — the ball is DEAD at op start, so a `reopen.pre`
            // plugin's §7 `current_state` is absent exactly as it is on `create`.
            // The op-start state is "there is no ball", and that is the truth.
            Ok(Some(Authored { base: Box::new(base), before: None, id }))
        }
        // The diffless verbs never reach run()'s mutating branch; reject defensively.
        _ => Err(other(format!("{}: not a mutating verb", verb.token()))),
    }
}

/// The §7 `command` — the op plus its body intent. `body_change` is the new
/// markdown ball body (`--body`) when the op rewrites it (§7); `message` is the
/// `-m` note, threaded for a close's delivery-message override (bl-b9a6).
/// Field-level changes are NOT carried here (single source of truth, bl-3bfd
/// §15): a plugin reads them from the change worktree / the `before`/`after`
/// states, not a second diff description. Its presence (vs the diffless `None`)
/// marks this a ball-mutating op (§7). `target` is the derived §11 delivery
/// target ([`crate::target::derive`]) — the dispatch computes it, this only
/// carries it onto the wire. `id` is the op's ball ([`Authored::id`]), riding
/// EVERY payload so identity is an op input rather than something a plugin
/// re-derives from the change worktree (§0 obligation 4; bl-a5f3).
pub(super) fn command(verb: Verb, flags: &Flags, target: Option<String>, id: String) -> Command {
    Command {
        op: verb.token().to_string(),
        id: Some(id),
        body_change: flags.body.clone(),
        message: flags.message.clone(),
        target,
    }
}

/// Reconstruct the ball `reopen` restores — and carry its TWO refusals, which
/// live here rather than at [`crate::change::Reopen::stage`] because only this
/// side can see history at all (a base change is git-free by construction) and
/// because a refusal here costs no change worktree and no plugin chain.
///
/// LIVE FIRST. `mint` re-rolls off the LIVE id set alone (§ id generation) —
/// "a dead incarnation's id is legally reused" — so a dead id may since have been
/// minted to an unrelated ball, and restoring over it would clobber a stranger.
/// Checking liveness before the walk is also what keeps the second message
/// honest: a live id that never died would otherwise be reported as naming
/// nothing.
///
/// Then the recency walk itself ([`crate::reads::resolve_dead`], the ONE
/// reconstruction path, §2/§9): the newest deletion's PARENT tree, which is
/// exactly the content `bl show <id>` already renders for a dead ball. `None`
/// means the id names nothing in this store, live or dead.
///
/// `clean` drops `claimant` — see [`crate::change::Reopen`] for why that is the
/// only field a close can falsify, and why it is opt-in.
fn restored(store: &Path, id: &str, clean: bool) -> io::Result<Task> {
    if exists(store, id) {
        return Err(other(format!(
            "reopen: {id} is live — it names a ball that is open right now, not a retired one. \
             A closed id is legally re-minted, so this may be a different ball entirely; \
             read it with `bl show {id}`"
        )));
    }
    let Some(dead) = crate::reads::resolve_dead(store, id)? else {
        return Err(other(format!(
            "reopen: {id} names nothing — no live ball, and no tasks/{id}.md deletion in this store's history"
        )));
    };
    let mut task = dead.task;
    if clean {
        task.claimant = None;
    }
    Ok(task)
}

/// The single positional `verb` expects (a `create` title, else a task id).
fn one_positional(flags: &Flags, verb: &str) -> io::Result<String> {
    match flags.positionals.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(crate::usage(format!("{verb}: expects exactly one positional argument"))),
    }
}
