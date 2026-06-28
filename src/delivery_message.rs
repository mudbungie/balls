//! The §11 delivery commit MESSAGE — carrying the author's rich `work/<id>`
//! context into the squash, not just the ball title (bl-b9a6).
//!
//! `bl close`'s squash used to label the delivery commit with the ball title
//! alone, so the multi-line rationale agents write on their work branch (the
//! project's "write rich commit bodies" rule, which release-plz renders into
//! the CHANGELOG) was dropped — it survived only in the work-branch reflog the
//! settled-branch prune then removes. [`compose`] is the single decision of
//! WHICH message the squash carries; [`deliver_close`] is the one close.pre
//! caller that reads the sources and squashes.

use std::io;

use crate::delivery::{Repo, Spec};

/// The §11 close.pre delivery (split from [`crate::delivery::dispatch`] so the
/// message policy sits beside [`compose`]): resolve the integration branch,
/// read the author's substantive `work/<id>` messages off it — BEFORE
/// [`Repo::deliver`] captures pending work or folds integration in, so neither
/// the ball-titled capture nor the reintegration merge commit pollutes them —
/// compose the delivery message, then squash.
pub fn deliver_close(repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    let integration = repo.integration()?;
    let work = repo.work_messages(spec.branch, &integration)?;
    let message = compose(spec.override_msg, &work, spec.subject, spec.marker);
    repo.deliver(spec.worktree, spec.branch, &integration, &message, spec.marker)
}

/// Pick the delivery commit message, highest precedence first:
///   1. an explicit `-m` on the close — a FULL override of subject + body;
///   2. else the author's substantive `work/<id>` messages — every NON-MERGE
///      commit since the branch forked, oldest-first, blank-line joined (the
///      `--no-merges` caller already drops the reintegration fold; a
///      multi-commit branch keeps ALL its rationale rather than electing one);
///   3. else the ball-title `subject` (the empty-deliverable / never-committed
///      fallback), which already carries the tag.
///
/// The `[id]` `marker` — the §7 delivery tag the rollback/release scan
/// ([`crate::delivery_repo::Project::marked`]) greps and the changelog reads —
/// is GUARANTEED on the subject line: an author message that omits it gets it
/// appended; one that already carries it is left untouched (no `[id] [id]`).
#[must_use]
pub fn compose(override_msg: Option<&str>, work: &[String], subject: &str, marker: &str) -> String {
    if let Some(m) = override_msg.map(str::trim).filter(|m| !m.is_empty()) {
        return with_marker(m, marker);
    }
    let parts: Vec<&str> = work.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).collect();
    if parts.is_empty() {
        return subject.to_string(); // fallback subject is already tagged
    }
    with_marker(&parts.join("\n\n"), marker)
}

/// Ensure `marker` rides `message`'s subject line, idempotently — the tag must
/// reach `git log`'s subject so the §7 scan and the changelog both see it,
/// without duplicating one the author already wrote anywhere in the message.
fn with_marker(message: &str, marker: &str) -> String {
    let message = message.trim_end();
    if message.contains(marker) {
        return message.to_string();
    }
    match message.split_once('\n') {
        Some((head, rest)) => format!("{head} {marker}\n{rest}"),
        None => format!("{message} {marker}"),
    }
}

#[cfg(test)]
#[path = "delivery_message_tests.rs"]
mod tests;
