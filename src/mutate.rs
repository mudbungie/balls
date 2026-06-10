//! §9 deliverable-verb dispatch — `create`/`claim`/`unclaim`/`update`/`close`/
//! `drop`, wired to the §8 engine. The MUTATING counterpart to [`crate::checkout`]
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

use crate::change::{Create, Occupancy, Retire, Update};
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
#[path = "mutate_report.rs"]
mod report;

/// Run a mutating verb (§9) end to end: parse `args`, author the verb's base
/// change against the STORE checkout, and seal it onto `tasks_branch` through the
/// §8 engine + the §6 plugin chain (resolved from the LANDING `plugins.toml`
/// `[hooks]` schedule, §2/§6). The
/// checkout must already be a landing (`bl prime` founds it, §12) — a mutating op
/// never bootstraps. `verb` is guaranteed mutating by the [`crate::run`] dispatch.
pub fn run(edge: &Edge, verb: Verb, args: &[String]) -> io::Result<()> {
    let flags = parse(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    if !landing.join("config").is_dir() {
        return Err(other("no balls checkout here — run `bl prime` first"));
    }

    let (base, before) = base_change(verb, &store, &flags, now())?;
    let cfg = EffectiveConfig::resolve(&landing, &edge.xdg.user_config())?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level));
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

/// Author the verb's [`BaseChange`] from the parsed `flags`, plus the ball's
/// op-start state (`before`, the §7 `current_state` a `pre` plugin sees — `None`
/// on `create`, which has no prior ball). `now` is injected, so the change stays
/// pure (it never reads a clock). Only the six mutating verbs reach here.
fn base_change(verb: Verb, store: &Path, flags: &Flags, now: i64) -> io::Result<(Box<dyn BaseChange>, Option<Task>)> {
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
            Ok((Box::new(base), None))
        }
        Verb::Claim | Verb::Unclaim => {
            build::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let claimant = (verb == Verb::Claim).then(|| actor.clone());
            let base = Occupancy { verb, id, claimant, actor, now, message: flags.message.clone() };
            Ok((Box::new(base), Some(before)))
        }
        Verb::Update => {
            build::forbid_foreign_blocks(flags, verb)?;
            build::forbid_contradictions(flags)?;
            let mut positionals = flags.positionals.iter();
            let id = positionals.next().ok_or_else(|| other("update: needs a task id"))?.clone();
            let before = read_task(store, &id)?;
            let edits = build::edits(positionals, flags)?;
            let base = Update { id, actor, now, edits, message: flags.message.clone() };
            Ok((Box::new(base), Some(before)))
        }
        Verb::Close | Verb::Drop => {
            build::forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let base = Retire { verb, id, title: before.title.clone(), actor, message: flags.message.clone() };
            Ok((Box::new(base), Some(before)))
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

/// The parsed front-door flags + positionals, verb-agnostic. The per-verb
/// `base_change` validates which it accepts. `message` is the `-m` §5 commit
/// narration (every verb); `body` is the `--body` ball markdown body
/// (create/update). EVERY ball field is overwriteable on `update` — there is no
/// create-only split: `title`/`parent`/`priority`/`tags`/extras set, and the
/// `--no-*` family clears (`no_parent`/`no_priority` clear the scalar,
/// `no_tags`/`no_needs` drop a member, a `key=` empty extra removes it). Only
/// `blocks` (a reciprocal edge on ANOTHER task) stays create-only.
#[derive(Debug, Default, PartialEq, Eq)]
struct Flags {
    actor: String,
    message: Option<String>,
    body: Option<String>,
    title: Option<String>,
    parent: Option<String>,
    no_parent: bool,
    blocks: Vec<String>,
    needs: Vec<String>,
    no_needs: Vec<String>,
    priority: Option<i64>,
    no_priority: bool,
    tags: Vec<String>,
    no_tags: Vec<String>,
    positionals: Vec<String>,
}

/// Parse argv into [`Flags`]. A leading-`-` token that is not a known flag is an
/// error; everything else is a positional. `--as` defaults to `default_actor`.
fn parse(args: &[String], default_actor: &str) -> io::Result<Flags> {
    let mut f = Flags { actor: default_actor.to_string(), ..Flags::default() };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => f.actor = value(args, &mut i, "--as")?,
            "-m" | "--message" => f.message = Some(value(args, &mut i, "-m")?),
            "--body" => f.body = Some(value(args, &mut i, "--body")?),
            "--title" => f.title = Some(value(args, &mut i, "--title")?),
            "--parent" => f.parent = Some(value(args, &mut i, "--parent")?),
            "--no-parent" => f.no_parent = true,
            "--blocks" => f.blocks.push(value(args, &mut i, "--blocks")?),
            "--needs" => f.needs.push(value(args, &mut i, "--needs")?),
            "--no-needs" => f.no_needs.push(value(args, &mut i, "--no-needs")?),
            "-p" | "--priority" => {
                let v = value(args, &mut i, "-p")?;
                f.priority = Some(v.parse().map_err(|_| other(format!("-p: '{v}' is not an integer")))?);
            }
            "--no-priority" => f.no_priority = true,
            "-t" | "--tag" => f.tags.push(value(args, &mut i, "-t")?),
            "--no-tag" => f.no_tags.push(value(args, &mut i, "--no-tag")?),
            flag if flag.starts_with('-') => return Err(other(format!("unexpected flag '{flag}'"))),
            _ => f.positionals.push(args[i].clone()),
        }
        i += 1;
    }
    Ok(f)
}

/// The value following a `--flag`, advancing the cursor; a missing value errors.
fn value(args: &[String], i: &mut usize, flag: &str) -> io::Result<String> {
    *i += 1;
    args.get(*i).cloned().ok_or_else(|| other(format!("{flag} needs a value")))
}

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
