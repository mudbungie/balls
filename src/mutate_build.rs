//! §9/§10 front-door flag translation — turn parsed [`super::Flags`] into the
//! `{id, on}` edges and [`FieldEdit`]s each verb authors, plus the per-verb
//! guards over which flags a verb accepts. Split out of [`crate::mutate`] so the
//! dispatch there stays orchestration; the edge meaning (`--needs`/`--blocks`/
//! `--no-needs`) lives here.
//!
//! `update` overwrites EVERY ball field (§9) — title, body, parent, priority,
//! tags, extras, and its own blocker edges — so there is no create-only split.
//! The set flags (`--title`/`--body`/`--parent`/`-p`/`-t`/`key=value`/`--needs`)
//! pair with a `--no-*` clear family (`--no-parent`/`--no-priority` blank a
//! scalar, `--no-tag`/`--no-needs` drop a member, a `key=` empty value removes
//! an extra). The §10 in-band blocker unlink keeps an enforced (and possibly
//! cyclic) edge recoverable without store-file surgery. ONLY `--blocks` (a
//! RECIPROCAL edge naming this task on ANOTHER) stays create-only — it is not
//! "this task's own" field, so re-wire it at create.

use std::io;
use std::path::Path;

use super::{other, Flags};
use crate::change::FieldEdit;
use crate::reads::resolve_dead;
use crate::task::{Blocker, On};
use crate::taskfile::exists;
use crate::verb::Verb;

/// §10 front-door edge validation (bl-6b8c): `--needs`/`--blocks` (and
/// `--subtask-of`'s reciprocal gate) REFUSE a target that is not LIVE, naming
/// the id and whether it is dead or unknown. Under fixed random ids a
/// never-minted target is always a typo or a hallucination — what it buys is a
/// silently ungated task — and a dead target is an edge born resolved: a
/// blocker that can never block. Only the write is validated (the store is
/// never audited — an existing dangling edge stays read-side inert); the
/// dead-vs-unknown naming rides the §9 recency walk, on the refusal path only.
pub(super) fn require_live<'a>(store: &Path, verb: Verb, targets: impl Iterator<Item = &'a str>) -> io::Result<()> {
    for id in targets {
        if exists(store, id) {
            continue;
        }
        let fate = if resolve_dead(store, id)?.is_some() {
            "already closed — a dead blocker can never block"
        } else {
            "not a known id"
        };
        return Err(other(format!("{}: edge target '{id}' is {fate}; drop the flag", verb.token())));
    }
    Ok(())
}

/// `--needs B[:OP]` → the task's own blockers: it can't make op `OP` until `B`
/// resolves. `OP` defaults to `claim` (a dependency, §10), so a bare `--needs B`
/// is the common "blocked from starting until B lands". Shared by `create`'s
/// authoring and `update`'s [`edits`] (post-hoc add, §10).
pub(super) fn needs_blockers(flags: &Flags) -> io::Result<Vec<Blocker>> {
    flags
        .needs
        .iter()
        .map(|spec| match spec.split_once(':') {
            Some((id, op)) => Ok(Blocker { id: id.to_string(), on: verb_of(op)? }),
            None => Ok(Blocker { id: spec.clone(), on: On::Claim }),
        })
        .collect()
}

/// The create-time parent, with the §10 `--subtask-of` sugar folded in:
/// `--subtask-of E` IS a parent spelling (`--parent E --blocks claim` in one
/// word), so naming both is a conflict, never a silent pick.
pub(super) fn effective_parent(flags: &Flags) -> io::Result<Option<String>> {
    if flags.subtask_of.is_some() && flags.parent.is_some() {
        return Err(other("create: --subtask-of and --parent conflict — --subtask-of IS a parent spelling (parent + claim-gate)"));
    }
    Ok(flags.subtask_of.clone().or_else(|| flags.parent.clone()))
}

/// `--blocks OP` / `--blocks ID:OP` → reciprocal edges naming THIS new task on a
/// target's op `OP`: a bare `OP` gates the [`effective_parent`] (required — that
/// is the only target a bare form has), an explicit `ID:OP` gates a non-parent.
/// This is the §10/§15 front door for the retired `--gates X` (= `--parent X
/// --blocks close`): containment never mints a blocker, so every gate is spelled
/// here. `--subtask-of E` contributes its `{child, claim}` gate on `E` — the
/// sugar's blocking half: it gates the epic's CLAIM (not close), so an epic
/// with open children derives as *blocked* and drops out of the ready set
/// (bl-5d9a) — deduped against an explicit equivalent.
pub(super) fn blocks_edges(flags: &Flags, parent: Option<&str>) -> io::Result<Vec<(String, On)>> {
    let mut edges: Vec<(String, On)> = flags
        .blocks
        .iter()
        .map(|spec| {
            if let Some((id, op)) = spec.split_once(':') {
                Ok((id.to_string(), verb_of(op)?))
            } else {
                let parent = parent.map(ToString::to_string).ok_or_else(|| {
                    other("create: --blocks OP needs --parent/--subtask-of; gate a non-parent with --blocks ID:OP")
                })?;
                Ok((parent, verb_of(spec)?))
            }
        })
        .collect::<io::Result<_>>()?;
    if let Some(e) = &flags.subtask_of {
        let gate = (e.clone(), On::Claim);
        if !edges.contains(&gate) {
            edges.push(gate);
        }
    }
    Ok(edges)
}

/// Resolve an op token (`claim`/`close`/`update`/…) to its [`Verb`] — `on` is ANY
/// op (§10/§15), so any known verb is a valid edge target.
fn verb_of(token: &str) -> io::Result<On> {
    Verb::parse(token).ok_or_else(|| other(format!("'{token}' is not a known op")))
}

/// Build the §9 `update` [`FieldEdit`] list — every ball field is overwriteable.
/// A trailing `key=value` positional sets a preserved `extra` (§3, the team-field
/// seam) and a bare `key=` (empty value) REMOVES it; `--title`/`--body` overwrite
/// title and the markdown body; `--parent`/`--no-parent` set or clear the parent
/// pointer; `-p`/`--no-priority` set or clear priority; `-t`/`--no-tag` add or
/// drop a tag; `--needs`/`--no-needs` add or unlink one of the task's own blocker
/// edges (§10). `--body` is the BALL body now, not the commit message (`-m`).
pub(super) fn edits<'a>(extras: impl Iterator<Item = &'a String>, flags: &Flags) -> io::Result<Vec<FieldEdit>> {
    let mut edits = Vec::new();
    for kv in extras {
        let (k, v) = kv.split_once('=').ok_or_else(|| other(format!("update: '{kv}' is not key=value")))?;
        edits.push(extra_edit(k, v)?);
    }
    if let Some(t) = &flags.title {
        edits.push(FieldEdit::Title(t.clone()));
    }
    if let Some(b) = &flags.body {
        edits.push(FieldEdit::Body(b.clone()));
    }
    if let Some(p) = &flags.parent {
        edits.push(FieldEdit::Parent(Some(p.clone())));
    } else if flags.no_parent {
        edits.push(FieldEdit::Parent(None));
    }
    if let Some(p) = flags.priority {
        edits.push(FieldEdit::Priority(Some(p)));
    } else if flags.no_priority {
        edits.push(FieldEdit::Priority(None));
    }
    edits.extend(flags.tags.iter().map(|t| FieldEdit::AddTag(t.clone())));
    edits.extend(flags.no_tags.iter().map(|t| FieldEdit::RemoveTag(t.clone())));
    edits.extend(needs_blockers(flags)?.into_iter().map(FieldEdit::AddBlocker));
    edits.extend(flags.no_needs.iter().map(|spec| {
        let id = spec.split_once(':').map_or(spec.as_str(), |(id, _)| id);
        FieldEdit::RemoveBlocker(id.to_string())
    }));
    Ok(edits)
}

/// The keys `key=value` refuses by name: facts whose one authoritative home is
/// not a preserved extra. `id` is the filename and `body` is the markdown after
/// the fence (the [`crate::task`] shadow keys — a stored line would be a lossy
/// shadow the bedrock projection drops, §3/§9); `created`/`updated` are the
/// seal's stamps and `root_commit` is the create-time repo identity (bl-1ce7),
/// so a hand-set value never survives (and would forge the claim guard, which
/// has no override). The string-typed §3 fields (`title=`/`claimant=`/`parent=`)
/// are NOT here: overwriting them is the update contract, just spelled without
/// the flag.
const RESERVED: [&str; 5] = ["id", "body", "created", "updated", "root_commit"];

/// A `key=value` extra edit: an empty `value` REMOVES the key (the §3 clear),
/// any other value sets it as a string. Setting an extra to "" is the degenerate
/// case removal takes precedence over — clear is the useful operation. A
/// [`RESERVED`] key is refused by name in both forms — there is never a
/// reserved-named extra to set or remove.
fn extra_edit(k: &str, v: &str) -> io::Result<FieldEdit> {
    if RESERVED.contains(&k) {
        return Err(other(format!(
            "update: '{k}' is reserved, not an extra — the id is the filename, the body is --body, created/updated are the seal's stamps, root_commit is the create-time repo identity"
        )));
    }
    Ok(if v.is_empty() {
        FieldEdit::RemoveExtra(k.to_string())
    } else {
        FieldEdit::SetExtra(k.to_string(), toml::Value::String(v.to_string()))
    })
}

// The per-verb flag-acceptance guards (forbid_*) live in a sibling of
// [`crate::mutate`], `mutate_guards.rs` — declared there, called as `guards::*`.
