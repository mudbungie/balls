//! §9 per-verb flag guards — which flags each verb accepts, rejected loudly (never
//! silently picked) before any authoring. Lifted from [`super::build`] so the
//! flag→edge translation there stays the edge meaning; this is the acceptance
//! envelope (`create` mints so has nothing to remove; the occupancy/retire verbs
//! shape no fields; `update --edit` is the whole-buffer either/or).

use std::io;

use super::Flags;
use crate::verb::Verb;

/// `update` edits this task's OWN fields and blockers; only `--blocks` (a
/// reciprocal edge naming this task on ANOTHER) remains create-only. `--parent`
/// is now an ordinary overwriteable field (set via `--parent`, clear via
/// `--no-parent`).
pub(super) fn forbid_foreign_blocks(flags: &Flags, verb: Verb) -> io::Result<()> {
    if !flags.blocks.is_empty() {
        return Err(crate::usage(format!(
            "{}: --blocks (a reciprocal edge on ANOTHER task) is create-only; update edits this task's own fields",
            verb.token()
        )));
    }
    if flags.subtask_of.is_some() {
        return Err(crate::usage(format!(
            "{}: --subtask-of carries a reciprocal claim-gate, so it is create-only; set --parent and gate with a fresh gate task",
            verb.token()
        )));
    }
    Ok(())
}

/// Reject an update that both sets and clears one scalar — `--parent`+`--no-parent`
/// or `-p`+`--no-priority` — rather than silently picking one.
pub(super) fn forbid_contradictions(flags: &Flags) -> io::Result<()> {
    if flags.parent.is_some() && flags.no_parent {
        return Err(crate::usage("update: --parent and --no-parent conflict"));
    }
    if flags.priority.is_some() && flags.no_priority {
        return Err(crate::usage("update: -p and --no-priority conflict"));
    }
    Ok(())
}

/// `create` mints a fresh ball, so the `--no-*` clear family and `--title` are
/// nonsensical: there is nothing to remove, and the title is the positional.
pub(super) fn forbid_removals_on_create(flags: &Flags) -> io::Result<()> {
    if flags.title.is_some() {
        return Err(crate::usage("create: the title is the positional argument, not --title"));
    }
    if flags.no_parent || flags.no_priority || !flags.no_tags.is_empty() || !flags.no_needs.is_empty() {
        return Err(crate::usage("create: --no-* removal flags are only for update — a fresh ball has nothing to remove"));
    }
    if flags.edit {
        return Err(crate::usage("create: --edit is update-only — a fresh ball has no stored buffer to edit"));
    }
    Ok(())
}

/// Does any field-setting flag carry a value? The shared predicate behind
/// [`forbid_shaping`] (verbs that shape nothing) and [`forbid_fields_with_edit`]
/// (`--edit` shapes everything at once); `--edit` itself is deliberately not in
/// here — it is the OTHER side of the either/or.
fn shapes(flags: &Flags) -> bool {
    flags.title.is_some()
        || flags.body.is_some()
        || flags.parent.is_some()
        || flags.subtask_of.is_some()
        || flags.no_parent
        || flags.no_priority
        || flags.priority.is_some()
        || !flags.blocks.is_empty()
        || !flags.needs.is_empty()
        || !flags.no_needs.is_empty()
        || !flags.tags.is_empty()
        || !flags.no_tags.is_empty()
}

/// The occupancy/retire verbs (`claim`/`unclaim`/`close`) shape no ball
/// fields: reject every field-edit flag — `--edit` (the whole-buffer shape)
/// included. Only the id, `--as`, and the `-m` commit narration are accepted.
pub(super) fn forbid_shaping(flags: &Flags, verb: Verb) -> io::Result<()> {
    if shapes(flags) || flags.edit {
        return Err(crate::usage(format!("{}: takes no field edits — only the id, --as, and -m", verb.token())));
    }
    Ok(())
}

/// `update --edit` and the field-setting flags would race over the same payload
/// (the buffer vs the flag values), so they are a clean either/or (§9): set
/// fields OR hand-edit, never both.
pub(super) fn forbid_fields_with_edit(flags: &Flags) -> io::Result<()> {
    if shapes(flags) {
        return Err(crate::usage("update: --edit and the field flags are mutually exclusive — hand-edit OR set fields"));
    }
    Ok(())
}
