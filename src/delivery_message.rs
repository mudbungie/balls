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
    let message = compose(spec.override_msg, &work, spec.subject);
    repo.deliver(spec.worktree, spec.branch, &integration, &message, spec.marker)
}

/// Compose the §11 delivery commit message. The SUBJECT line is ALWAYS the
/// §7-wire ball title tagged `[id]` (`subject`, built by
/// [`crate::delivery_path::subject`]) — there is no subject override (§5),
/// exactly as the store seal's `commit_message` fixes the subject from the ball
/// `title` with `-m` a SEPARATE body. Everything the author writes is BODY,
/// under that one subject — the close's `-m` narration first (when given), then
/// the author's substantive `work/<id>` messages (every NON-MERGE commit since
/// the branch forked, oldest-first; the `--no-merges` caller already drops the
/// reintegration fold), all blank-line joined. Both go in the body TOGETHER —
/// neither elects the other out, so bl-b9a6's rich work context survives even
/// when `-m` is given. An empty deliverable (no `-m`, never committed) is the
/// bare tagged subject.
///
/// The `[id]` delivery tag the rollback/release scan
/// ([`crate::delivery_repo::Project::marked`]) greps and the changelog reads
/// therefore rides the subject unconditionally — `subject` already carries it,
/// and the subject is never displaced by author text.
#[must_use]
pub fn compose(override_msg: Option<&str>, work: &[String], subject: &str) -> String {
    let body: Vec<&str> = override_msg
        .into_iter()
        .chain(work.iter().map(String::as_str))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if body.is_empty() {
        return subject.to_string(); // empty deliverable: bare tagged subject
    }
    format!("{subject}\n\n{}", body.join("\n\n"))
}

#[cfg(test)]
#[path = "delivery_message_tests.rs"]
mod tests;
