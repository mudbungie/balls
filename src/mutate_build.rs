//! §9/§10 front-door flag translation — turn parsed [`super::Flags`] into the
//! `{id, on}` edges and [`FieldEdit`]s each verb authors, plus the per-verb
//! guards over which flags a verb accepts. Split out of [`crate::mutate`] so the
//! dispatch there stays orchestration; the edge meaning (`--needs`/`--blocks`/
//! `--no-needs`) lives here.
//!
//! Two flags carry blocker EDITS that survive create (§10): `--needs B[:OP]` adds
//! one of THIS task's own blockers, `--no-needs B` drops it. They are the only
//! relational edit a live task takes — the §10 in-band unlink that keeps an
//! enforced (and possibly cyclic) blocker recoverable without store-file surgery.
//! `--parent`/`--blocks` stay create-only: containment and a RECIPROCAL edge on
//! ANOTHER task are not "this task's own" edges (re-wire those at create).

use std::io;

use super::{other, Flags};
use crate::change::FieldEdit;
use crate::task::{Blocker, On};
use crate::verb::Verb;

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

/// `--blocks OP` / `--blocks ID:OP` → reciprocal edges naming THIS new task on a
/// target's op `OP`: a bare `OP` gates the `--parent` (required — that is the
/// only target a bare form has), an explicit `ID:OP` gates a non-parent. This is
/// the §10/§15 front door for the retired `--gates X` (= `--parent X --blocks
/// close`): containment never mints a blocker, so every gate is spelled here.
pub(super) fn blocks_edges(flags: &Flags) -> io::Result<Vec<(String, On)>> {
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

/// Build the §9 `update` [`FieldEdit`] list: each trailing `key=value` positional
/// reaches a preserved `extra` field (§3, the team-field seam), `-p`/`-t` re-set
/// priority and add tags, `--needs`/`--no-needs` add/drop one of the task's own
/// blocker edges (§10). `--body`/`-m` ride the commit message, not the ball.
pub(super) fn edits<'a>(extras: impl Iterator<Item = &'a String>, flags: &Flags) -> io::Result<Vec<FieldEdit>> {
    let mut edits = Vec::new();
    for kv in extras {
        let (k, v) = kv.split_once('=').ok_or_else(|| other(format!("update: '{kv}' is not key=value")))?;
        edits.push(FieldEdit::SetExtra(k.to_string(), toml::Value::String(v.to_string())));
    }
    if let Some(p) = flags.priority {
        edits.push(FieldEdit::Priority(Some(p)));
    }
    edits.extend(flags.tags.iter().map(|t| FieldEdit::AddTag(t.clone())));
    edits.extend(needs_blockers(flags)?.into_iter().map(FieldEdit::AddBlocker));
    edits.extend(flags.no_needs.iter().map(|spec| {
        let id = spec.split_once(':').map_or(spec.as_str(), |(id, _)| id);
        FieldEdit::RemoveBlocker(id.to_string())
    }));
    Ok(edits)
}

/// `update` edits this task's OWN fields and blockers; `--parent` (containment)
/// and `--blocks` (a reciprocal edge on ANOTHER task) remain create-only.
pub(super) fn forbid_foreign_edges(flags: &Flags, verb: Verb) -> io::Result<()> {
    if flags.parent.is_some() || !flags.blocks.is_empty() {
        return Err(other(format!(
            "{}: --parent/--blocks are create-only; update edits this task's own blockers via --needs/--no-needs",
            verb.token()
        )));
    }
    Ok(())
}

/// The occupancy/retire verbs (`claim`/`unclaim`/`close`/`drop`) shape no fields:
/// reject every edge flag plus `-p`/`-t`.
pub(super) fn forbid_shaping(flags: &Flags, verb: Verb) -> io::Result<()> {
    if flags.parent.is_some() || !flags.blocks.is_empty() || !flags.needs.is_empty() || !flags.no_needs.is_empty() {
        return Err(other(format!("{}: --parent/--blocks/--needs/--no-needs are only for create/update", verb.token())));
    }
    if flags.priority.is_some() || !flags.tags.is_empty() {
        return Err(other(format!("{}: -p/-t are only for create/update", verb.token())));
    }
    Ok(())
}
