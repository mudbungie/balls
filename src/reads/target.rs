//! The rendered delivery-target column (bl-6915) — the read-side projection of
//! [`crate::target`].
//!
//! Under nesting a closed child is DELIVERED to its target ref, not LANDED on
//! the integration branch, and absence is a closed ball's whole record. At epic
//! scale (hours) that is benign; at release scale (weeks) a ball can be closed
//! and invisible for a month. The answer is NOT a stored field — it is a
//! rendered column, exactly as the root-aware `--everywhere` project labels are
//! render-only decoration over a derived fact (bl-0161, [`super::scope`]).
//! Projection grows, schema does not: `--json` stays the bedrock mirror of
//! stored frontmatter and carries no target.
//!
//! **The column IS the landed-vs-delivered marker — no git query, at any
//! depth.** A target derives only against a LIVE parent ([`crate::target`]: a
//! parent whose file is gone gates nothing), so for a CLOSED ball the rendered
//! target is precisely the "delivered, not landed" signal, and its absence is
//! "landed". Where the work went is then the ordinary graph read — follow the
//! target ball, which renders its own target, up to the parentless ball whose
//! target is the integration branch. A `git merge-base --is-ancestor` against a
//! delivery tag would answer the same question by re-deriving the delivery
//! plugin's tag naming inside core, per row, having already been answered by the
//! ball graph balls owns.
//!
//! The one deviation from [`crate::target::derive`]: the live parent comes from
//! the already-loaded [`Catalog`] rather than a fresh `read_task`, so a listing
//! pays no IO per row. The DECISION has a single home either way — both call
//! [`close_gated`].

use super::Catalog;
use crate::target::close_gated;
use crate::task::Task;

/// The id of the ball `id`'s work delivers into — its `parent`, when that parent
/// is live in `cat` AND close-gated on `id` (the §11 nesting declaration).
/// `None` ⇒ the integration branch, the flat case.
pub(crate) fn of<'a>(cat: &'a Catalog, id: &str, task: &Task) -> Option<&'a str> {
    let parent = task.parent.as_deref()?;
    let live = cat.get(parent)?;
    close_gated(id, &live.task).then_some(live.id.as_str())
}

/// The trailing `  ->bl-xxxx` marker a human `bl list` row hangs on a ball with
/// a delivery target, `""` for the flat case (the integration branch is the
/// unmarked default, so a flat listing reads exactly as it always did). Carries
/// its own leading spacing, like the fleet-view label it mirrors.
pub(crate) fn row_marker(cat: &Catalog, id: &str, task: &Task) -> String {
    of(cat, id, task).map_or_else(String::new, |t| format!("  ->{t}"))
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
