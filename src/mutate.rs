//! §9 deliverable-verb dispatch — `create`/`claim`/`unclaim`/`update`/`close`/
//! `drop`, wired to the §8 engine. The MUTATING counterpart to [`crate::checkout`]
//! (which wires the diffless `prime`/`sync`): these author a `tasks/<id>.md` diff
//! and SEAL it, so they run the full Author → Pre → Seal → Post → Teardown shape
//! against a change worktree off the STORE terminus
//! ([`crate::lifecycle::Engine::seal`]).
//!
//! Every collaborator already exists — [`crate::change`] authors each verb's diff
//! ([`BaseChange`]), [`crate::lifecycle`] runs the shape with §14 rollback,
//! [`crate::plugin`] is the §6 subprocess chain over the §7 [`crate::wire`]. This
//! is the integration seam: it parses argv into a [`BaseChange`], resolves the §7
//! binding + the registry's `NN-` plugin sets, INJECTS the clock, and drives the
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
use crate::id::IdScheme;
use crate::lifecycle::{BaseChange, Engine};
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::task::{Blocker, On, Task};
use crate::taskfile::{read_task, task_ids};
use crate::verb::Verb;
use crate::wire::{Command, OpContext};

/// Run a mutating verb (§9) end to end: parse `args`, author the verb's base
/// change against the STORE checkout, and seal it onto `tasks_branch` through the
/// §8 engine + the §6 plugin chain (resolved from the LANDING registry, §2). The
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
    let remote = checkout::origin_of(&landing);
    let binding = checkout::binding(&landing, &store, &edge.invocation_path, remote, cfg.tasks_branch);
    let ctx = OpContext {
        actor: flags.actor.clone(),
        binding,
        command: Some(command(verb, &flags)),
        before,
        after: None,
    };

    let reg = Registry::at(&landing);
    let pre = reg.resolve(verb.token(), "pre")?;
    let post = reg.resolve(verb.token(), "post")?;
    let change_dir = clone.change(&change_token());
    let plugins = Subprocess::new(ctx, &clone.root().join("logs"), edge.depth);
    let terminus = Git::at(&store);
    Engine::new(&terminus, &plugins)
        .seal(base.as_ref(), verb, &change_dir, &pre, &post)
        .map(|_sha| ())
        .map_err(|e| other(e.to_string()))
}

/// Author the verb's [`BaseChange`] from the parsed `flags`, plus the ball's
/// op-start state (`before`, the §7 `current_state` a `pre` plugin sees — `None`
/// on `create`, which has no prior ball). `now` is injected, so the change stays
/// pure (it never reads a clock). Only the six mutating verbs reach here.
fn base_change(verb: Verb, store: &Path, flags: &Flags, now: i64) -> io::Result<(Box<dyn BaseChange>, Option<Task>)> {
    let actor = flags.actor.clone();
    match verb {
        Verb::Create => {
            let title = one_positional(flags, "create")?;
            let base = Create {
                id: IdScheme::default().generate(),
                actor,
                now,
                title,
                parent: flags.parent.clone(),
                priority: flags.priority,
                tags: flags.tags.clone(),
                blockers: needs_blockers(flags)?,
                blocks: blocks_edges(flags)?,
                over: flags.over.clone(),
                body: flags.body.clone(),
                existing: task_ids(store)?,
            };
            Ok((Box::new(base), None))
        }
        Verb::Claim | Verb::Unclaim => {
            forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let claimant = (verb == Verb::Claim).then(|| actor.clone());
            let base = Occupancy { verb, id, claimant, actor, now, over: flags.over.clone(), body: flags.body.clone() };
            Ok((Box::new(base), Some(before)))
        }
        Verb::Update => {
            forbid_structure(flags, verb)?;
            let mut positionals = flags.positionals.iter();
            let id = positionals.next().ok_or_else(|| other("update: needs a task id"))?.clone();
            let before = read_task(store, &id)?;
            let base = Update { id, actor, now, edits: edits(positionals, flags)?, over: flags.over.clone(), body: flags.body.clone() };
            Ok((Box::new(base), Some(before)))
        }
        Verb::Close | Verb::Drop => {
            forbid_shaping(flags, verb)?;
            let id = one_positional(flags, verb.token())?;
            let before = read_task(store, &id)?;
            let base = Retire { verb, id, title: before.title.clone(), actor, over: flags.over.clone(), body: flags.body.clone() };
            Ok((Box::new(base), Some(before)))
        }
        // The diffless verbs never reach run()'s mutating branch; reject defensively.
        _ => Err(other(format!("{}: not a mutating verb", verb.token()))),
    }
}

/// The §7 `command` — the op plus its body intent. Field-level changes are NOT
/// duplicated here (single source of truth): a plugin reads them from the change
/// worktree / the `before`/`after` states, not a second diff description. Its
/// presence (vs the diffless `None`) marks this as a ball-mutating op (§7).
fn command(verb: Verb, flags: &Flags) -> Command {
    Command { op: verb.token().to_string(), field_changes: Vec::new(), body_change: flags.body.clone() }
}

/// Build the §9 `update` [`FieldEdit`] list: each trailing `key=value` positional
/// reaches a preserved `extra` field (§3, the team-field seam), `-p`/`-t` re-set
/// priority and add tags. `--body`/`-m` ride the commit message, not the ball.
fn edits<'a>(extras: impl Iterator<Item = &'a String>, flags: &Flags) -> io::Result<Vec<FieldEdit>> {
    let mut edits = Vec::new();
    for kv in extras {
        let (k, v) = kv.split_once('=').ok_or_else(|| other(format!("update: '{kv}' is not key=value")))?;
        edits.push(FieldEdit::SetExtra(k.to_string(), toml::Value::String(v.to_string())));
    }
    if let Some(p) = flags.priority {
        edits.push(FieldEdit::Priority(Some(p)));
    }
    edits.extend(flags.tags.iter().map(|t| FieldEdit::AddTag(t.clone())));
    Ok(edits)
}

/// `--needs B[:OP]` → the new task's own blockers: it can't make op `OP` until
/// `B` resolves. `OP` defaults to `claim` (a dependency, §10), so a bare `--needs
/// B` is the common "blocked from starting until B lands".
fn needs_blockers(flags: &Flags) -> io::Result<Vec<Blocker>> {
    flags
        .needs
        .iter()
        .map(|spec| match spec.split_once(':') {
            Some((id, op)) => Ok(Blocker { id: id.to_string(), on: verb_of(op)? }),
            None => Ok(Blocker { id: spec.clone(), on: On::Claim }),
        })
        .collect()
}

/// `--blocks OP` / `--blocks ID:OP` → reciprocal edges naming THIS new task on a
/// target's op `OP`: a bare `OP` gates the `--parent` (required — that is the
/// only target a bare form has), an explicit `ID:OP` gates a non-parent. This is
/// the §10/§15 front door for the retired `--gates X` (= `--parent X --blocks
/// close`): containment never mints a blocker, so every gate is spelled here.
fn blocks_edges(flags: &Flags) -> io::Result<Vec<(String, On)>> {
    flags
        .blocks
        .iter()
        .map(|spec| {
            if let Some((id, op)) = spec.split_once(':') {
                Ok((id.to_string(), verb_of(op)?))
            } else {
                let parent = flags.parent.clone().ok_or_else(|| {
                    other("create: --blocks OP needs --parent; gate a non-parent with --blocks ID:OP")
                })?;
                Ok((parent, verb_of(spec)?))
            }
        })
        .collect()
}

/// Resolve an op token (`claim`/`close`/`update`/…) to its [`Verb`] — `on` is ANY
/// op (§10/§15), so any known verb is a valid edge target.
fn verb_of(token: &str) -> io::Result<On> {
    Verb::parse(token).ok_or_else(|| other(format!("'{token}' is not a known op")))
}

/// The single positional `verb` expects (a `create` title, else a task id).
fn one_positional(flags: &Flags, verb: &str) -> io::Result<String> {
    match flags.positionals.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(other(format!("{verb}: expects exactly one positional argument"))),
    }
}

/// `--parent`/`--blocks`/`--needs` are `create`'s front-door flags only.
fn forbid_structure(flags: &Flags, verb: Verb) -> io::Result<()> {
    if flags.parent.is_some() || !flags.blocks.is_empty() || !flags.needs.is_empty() {
        return Err(other(format!("{}: --parent/--blocks/--needs are only for create", verb.token())));
    }
    Ok(())
}

/// The occupancy/retire verbs shape no fields: reject structure plus `-p`/`-t`.
fn forbid_shaping(flags: &Flags, verb: Verb) -> io::Result<()> {
    forbid_structure(flags, verb)?;
    if flags.priority.is_some() || !flags.tags.is_empty() {
        return Err(other(format!("{}: -p/-t are only for create/update", verb.token())));
    }
    Ok(())
}

/// The parsed front-door flags + positionals, verb-agnostic. The per-verb
/// `base_change` validates which it accepts; `over`/`body` are the §5 message's
/// subject override and body, shared by every verb.
#[derive(Debug, Default, PartialEq, Eq)]
struct Flags {
    actor: String,
    over: Option<String>,
    body: Option<String>,
    parent: Option<String>,
    blocks: Vec<String>,
    needs: Vec<String>,
    priority: Option<i64>,
    tags: Vec<String>,
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
            "-m" | "--message" => f.over = Some(value(args, &mut i, "-m")?),
            "--body" => f.body = Some(value(args, &mut i, "--body")?),
            "--parent" => f.parent = Some(value(args, &mut i, "--parent")?),
            "--blocks" => f.blocks.push(value(args, &mut i, "--blocks")?),
            "--needs" => f.needs.push(value(args, &mut i, "--needs")?),
            "-p" | "--priority" => {
                let v = value(args, &mut i, "-p")?;
                f.priority = Some(v.parse().map_err(|_| other(format!("-p: '{v}' is not an integer")))?);
            }
            "-t" | "--tag" => f.tags.push(value(args, &mut i, "-t")?),
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
