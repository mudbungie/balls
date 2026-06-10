//! §8.3 seal validation (bl-528c) — "balls SEALS — validate, commit".
//!
//! Each verb's [`super::BaseChange::finalize`] checks only the op's OWN shape
//! (create: exactly one new file, re-read for the message). A `pre` plugin
//! edits the SHARED change worktree, so it can also touch a SIBLING
//! `tasks/*.md` — and a sibling edit nothing re-reads would seal unvalidated,
//! poisoning the store with a file no read can parse. The guard is one
//! invariant, not an op-apiece check: every CHANGED ball file must still parse
//! before the seal commits it. A deletion (close) has nothing to parse; a
//! non-ball path carries no §3 schema. The refusal names the file and the last
//! `pre` plugin that ran — the likeliest author of the corruption.

use std::fs;
use std::path::Path;

use crate::git::Anvil;
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::task::Task;

use super::OpError;

/// Refuse the seal unless every changed `tasks/*.md` under `dir` still parses.
/// `ran` is the engine's trace — its last `pre` entry is named in the refusal.
pub(super) fn changed_balls(anvil: &dyn Anvil, dir: &Path, ran: &[(PluginRef, Phase)]) -> Result<(), OpError> {
    for file in anvil.changed(dir).map_err(OpError::Anvil)? {
        if file.strip_prefix("tasks/").and_then(|f| f.strip_suffix(".md")).is_none() {
            continue; // only ball files carry the §3 schema
        }
        // A deleted ball (close) has nothing left to parse.
        let Ok(text) = fs::read_to_string(dir.join(&file)) else { continue };
        if let Err(e) = Task::parse(&text) {
            return Err(OpError::Invalid(refusal(&file, &e.to_string(), ran)));
        }
    }
    Ok(())
}

/// Render the refusal: the unparseable file, the parse error, and the last
/// `pre` plugin that ran (when one did — the base changes author only valid
/// balls, so a plugin-free corruption is the exotic case).
fn refusal(file: &str, error: &str, ran: &[(PluginRef, Phase)]) -> String {
    let last_pre = ran.iter().rev().find(|(_, ph)| *ph == Phase::Pre).map(|(p, _)| p.name.as_str());
    match last_pre {
        Some(name) => format!("seal refused: {file} no longer parses after pre plugin {name}: {error}"),
        None => format!("seal refused: {file} does not parse: {error}"),
    }
}
