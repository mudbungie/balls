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

/// The body's byte budget (bl-a500). The transport is stdin, so this is NOT a
/// platform limit — it is the ceiling past which a commit message stops being a
/// message. It has to exist because no graph bound can shrink the set: the body
/// is the commits on `work/<id>` since it forked, and `A..B` is ALREADY
/// `merge-base(A,B)..B`, so when integration is force-push REWRITTEN under a
/// live work branch the orphaned upstream history becomes indistinguishable
/// from authored work (~120 commits, 142 KB, observed). Past the budget the
/// body is cut on a whole-message boundary and SAYS how many it dropped — an
/// honest short message beats an unreadable one, and beats a crash.
const BODY_CAP: usize = 64 * 1024;

/// A message's SUBJECT LINE — everything git shows as `%s`, and the only part
/// of a delivery message that belongs on a command line or in a reflog (both
/// are single-line by construction). The one spelling, so the argv/reflog
/// bound is not re-derived at each use (bl-a500).
pub(crate) fn subject_line(message: &str) -> &str {
    message.lines().next().unwrap_or(message)
}

/// The §11 close.pre delivery (split from [`crate::delivery::dispatch`] so the
/// message policy sits beside [`compose`]): resolve the delivery TARGET ref
/// ([`crate::delivery::target_branch`] — the nesting parent's `work/<id>` when
/// the ball nests, else the integration branch, bl-7b71),
/// read the author's substantive `work/<id>` messages off it — BEFORE
/// [`Repo::deliver`] captures pending work, so the ball-titled capture commit
/// does not pollute them (the closer's own reconciling merges are already
/// dropped by `--no-merges`) — compose the delivery message, then squash.
pub fn deliver_close(repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    let integration = crate::delivery::target_branch(repo, spec.target)?;
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
/// closer's reconciling merges), all blank-line joined. Both go in the body TOGETHER —
/// neither elects the other out, so bl-b9a6's rich work context survives even
/// when `-m` is given. An empty deliverable (no `-m`, never committed) is the
/// bare tagged subject.
///
/// The `[id]` delivery tag the rollback/release scan
/// ([`crate::delivery_repo::Project::marked`]) greps and the changelog reads
/// therefore rides the subject unconditionally — `subject` already carries it,
/// and the subject is never displaced by author text.
///
/// Two things are NOT body (bl-a500). A part that IS the subject adds nothing —
/// that is balls' own capture commit
/// ([`crate::delivery_repo::Project::capture`]) read back after an aborted
/// close, so dropping it makes the composition idempotent across retries.
/// And the body stops at [`BODY_CAP`], on a whole-message boundary, saying how
/// many it dropped.
#[must_use]
pub fn compose(override_msg: Option<&str>, work: &[String], subject: &str) -> String {
    let parts: Vec<&str> = override_msg
        .into_iter()
        .chain(work.iter().map(String::as_str))
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != subject)
        .collect();
    let mut body: Vec<&str> = Vec::new();
    let mut used = 0;
    for part in &parts {
        used += part.len() + 2; // the "\n\n" join
        if used > BODY_CAP {
            break;
        }
        body.push(part);
    }
    let dropped = parts.len() - body.len();
    let note = (dropped > 0)
        .then(|| format!("and {dropped} more work commit message(s), over the {BODY_CAP}-byte body budget"));
    let body: Vec<&str> = body.into_iter().chain(note.as_deref()).collect();
    if body.is_empty() {
        return subject.to_string(); // empty deliverable: bare tagged subject
    }
    format!("{subject}\n\n{}", body.join("\n\n"))
}

#[cfg(test)]
#[path = "delivery_message_tests.rs"]
mod tests;
