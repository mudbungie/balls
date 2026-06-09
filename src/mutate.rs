//! §9 deliverable-verb dispatch — `create`/`claim`/`unclaim`/`update`/`close`,
//! wired to the §8 engine. The MUTATING counterpart to [`crate::checkout`]
//! (which wires the diffless `prime`/`sync`): these author a `tasks/<id>.md` diff
//! and SEAL it, so they run the full Author → Pre → Seal → Post → Teardown shape
//! against a change worktree off the STORE anvil
//! ([`crate::lifecycle::Engine::seal`]).
//!
//! Every collaborator already exists — [`crate::change`] authors each verb's diff
//! ([`BaseChange`]), [`crate::lifecycle`] runs the shape with §14 rollback,
//! [`crate::plugin`] is the §6 subprocess chain over the §7 [`crate::wire`]. This
//! is the integration seam: it parses argv into a [`BaseChange`], resolves the §7
//! binding + the `[hooks]` plugin sets, INJECTS the clock, and drives the
//! engine. The §10/§15 front-door flags (`--parent` containment-only, `--blocks
//! OP`/`--blocks ID:OP`, `--needs B[:OP]`) write their `{id,on}` edges through
//! [`Create`]'s authoring — `on` is ANY op; all flag parsing is core — plugins
//! are hook binaries and never extend the parser (§10).

use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::change::{Create, FieldEdit, Occupancy, Retire, Update};
use crate::checkout;
use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::git::Git;
use crate::hooks::Hooks;
use crate::id::IdScheme;
use crate::lifecycle::{BaseChange, Engine};
use crate::log::{self, Level, Log};
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::task::Task;
use crate::taskfile::{read_task, task_ids};
use crate::verb::Verb;
use crate::wire::{Command, OpContext};

#[path = "mutate_build.rs"]
mod build;
#[path = "mutate_edit.rs"]
mod edit;
#[path = "mutate_report.rs"]
mod report;

/// Run a mutating verb (§9) end to end: parse `args`, author the verb's base
/// change against the STORE checkout, and seal it onto `tasks_branch` through the
/// §8 engine + the §6 plugin chain (resolved from the LANDING `plugins.toml`
/// `[hooks]` schedule, §2/§6). The
/// checkout must already be a landing (`bl prime` founds it, §12) — a mutating op
/// never bootstraps. `verb` is guaranteed mutating by the [`crate::run`] dispatch.
/// The one host seam read here is the [`edit::Editor`] (`--edit`'s env + tty +
/// prompt input), so [`dispatch`] below stays fully injectable for tests.
pub fn run(edge: &Edge, verb: Verb, args: &[String]) -> io::Result<()> {
    dispatch(edge, verb, args, &mut edit::Editor::live())
}

/// [`run`] with the `--edit` host seam injected. An authored change is sealed;
/// a `None` from [`base_change`] (`--edit` returned an unchanged buffer) is the
/// idempotent no-op — announced there, nothing to seal here.
fn dispatch(edge: &Edge, verb: Verb, args: &[String], editor: &mut edit::Editor) -> io::Result<()> {
    let flags = parse(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    if !landing.join("config").is_dir() {
        return Err(other("no balls checkout here — run `bl prime` first"));
    }

    let Some((base, before)) = base_change(verb, &store, &flags, now(), editor)? else {
        return Ok(());
    };
    let cfg = EffectiveConfig::resolve(&landing, &edge.xdg.user_config())?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level))?;
    let log = Log::new(clone.op_log(), level, verb, log::wall);
    // A mutating op takes no `--remote`; core reads only the explicit XDG remote
    // (§0 stays local), and the tracker discovers the project `origin` when it is
    // `None` (the bottom §12 tier — remote-talk, the tracker's alone).
    let remote = crate::config::xdg_remote(&edge.xdg.user_config());
    let binding = checkout::binding(&landing, &store, &edge.invocation_path, remote, cfg.tasks_branch);
    let ctx = OpContext {
        actor: flags.actor.clone(),
        binding,
        command: Some(command(verb, &flags)),
        before,
    };

    let hooks = Hooks::effective(&landing, &edge.xdg.user_config())?;
    let reg = Registry::at(&landing);
    let pre = hooks.resolve(&reg, verb.token(), "pre");
    let post = hooks.resolve(&reg, verb.token(), "post");
    let change_dir = clone.change(&change_token());
    let plugins = Subprocess::new(ctx, &log, edge.depth);
    let anvil = Git::at(&store);
    let sha = Engine::new(&anvil, &plugins, &log)
        .seal(base.as_ref(), verb, &change_dir, &pre, &post)
        .map_err(|e| other(e.to_string()))?;
    report::emit(verb, &store, &sha)
}

/// A verb's authored change plus the ball's op-start state (the §7
/// `current_state` a `pre` plugin sees — `None` on `create`, which has no prior
/// ball).
type Authored = (Box<dyn BaseChange>, Option<Task>);

/// Author the verb's [`BaseChange`] from the parsed `flags` (see [`Authored`]).
/// `now` is injected, so the change stays pure (it never reads a clock); the
/// `editor` seam serves only `update --edit`. `Ok(None)` is `--edit`'s
/// unchanged-buffer no-op — there is nothing to author. Only the five mutating
/// verbs reach here.
fn base_change(
    verb: Verb,
    store: &Path,
    flags: &Flags,
    now: i64,
    editor: &mut edit::Editor,
) -> io::Result<Option<Authored>> {
    let actor = flags.actor.clone();
    match verb {
        Verb::Create => {
            build::forbid_removals_on_create(flags)?;
            let title = one_positional(flags, "create")?;
            let base = Create {
                id: IdScheme::default().generate(),
                actor,
                now,
                title,
                parent: flags.parent.clone(),
                priority: flags.priority,
                tags: flags.tags.clone(),
                blockers: build::needs_blockers(flags)?,
                blocks: build::blocks_edges(flags)?,
                body: flags.body.clone(),
                message: flags.message.clone(),
                existing: task_ids(store)?,
            };
            Ok(Some((Box::new(base), None)))
        }
        Verb::Claim | Verb::Unclaim => {
            build::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let claimant = (verb == Verb::Claim).then(|| actor.clone());
            let base = Occupancy { verb, id, claimant, actor, now, message: flags.message.clone() };
            Ok(Some((Box::new(base), Some(before))))
        }
        Verb::Update => {
            build::forbid_foreign_blocks(flags, verb)?;
            build::forbid_contradictions(flags)?;
            let mut positionals = flags.positionals.iter();
            let id = positionals.next().ok_or_else(|| other("update: needs a task id"))?.clone();
            let before = read_task(store, &id)?;
            let edits = if flags.edit {
                // `--edit`: the buffer IS the payload — field flags and key=value
                // extras would race over it, so they are mutually exclusive (§9).
                build::forbid_fields_with_edit(flags)?;
                if positionals.next().is_some() {
                    return Err(other("update: --edit and key=value extras are mutually exclusive — the buffer is the payload"));
                }
                let Some(after) = editor.edited(&before, &id)? else { return Ok(None) };
                vec![FieldEdit::Replace(Box::new(after))]
            } else {
                build::edits(positionals, flags)?
            };
            let base = Update { id, actor, now, edits, message: flags.message.clone() };
            Ok(Some((Box::new(base), Some(before))))
        }
        Verb::Close => {
            build::forbid_shaping(flags, verb)?;
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
fn command(verb: Verb, flags: &Flags) -> Command {
    Command { op: verb.token().to_string(), body_change: flags.body.clone() }
}

/// The single positional `verb` expects (a `create` title, else a task id).
fn one_positional(flags: &Flags, verb: &str) -> io::Result<String> {
    match flags.positionals.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(other(format!("{verb}: expects exactly one positional argument"))),
    }
}

// The argv→[`Flags`] front-door parse lives in a sibling module (the §9 flag
// vocabulary in one place); re-imported so the dispatch reads naturally.
#[path = "mutate_args.rs"]
mod args;
use args::{parse, Flags};

/// The SOLE site that reads the wall clock and reduces it to §3 unix seconds.
/// Injected into each [`BaseChange`] so `change.rs` stays a pure, clock-free unit
/// (`now` is a plain argument there). A pre-epoch clock — never, in practice —
/// reads 0.
fn now() -> i64 {
    #[allow(clippy::cast_possible_wrap)]
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A unique name for the ephemeral change worktree (§8/§1 — nothing keys off it),
/// drawn from the same entropy [`IdScheme`] mints ids with, so the dispatch needs
/// no second randomness primitive.
fn change_token() -> String {
    IdScheme { prefix: String::new(), length: 32, alphabet: "0123456789abcdef".to_string() }.generate()
}

/// An ad-hoc op error.
fn other(msg: impl Into<String>) -> io::Error {
    io::Error::other(msg.into())
}

#[cfg(test)]
#[path = "mutate_tests.rs"]
mod tests;
