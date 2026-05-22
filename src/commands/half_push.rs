//! SPEC §7.4 half-push detection and retraction.
//!
//! A half-push is a task whose close commit landed on the state
//! branch but whose corresponding `[bl-xxxx]` delivery tag is not
//! reachable from main. This module scans the state branch for them
//! and provides the write path for the `forget-half-push` retraction
//! marker.

use balls::error::Result;
use balls::store::Store;
use balls::{git, git_state};

/// Scan the state branch for tasks that went through review (a
/// `state: review bl-xxxx` commit exists) and are now closed
/// (`state: close bl-xxxx`), but whose corresponding `[bl-xxxx]`
/// delivery tag is not reachable from main. Each hit is a half-push:
/// the state branch landed but the feature commit never did. Tasks
/// closed via `bl update status=closed` without ever being reviewed
/// are excluded — they legitimately never produce a main commit.
/// `state: review bl-xxxx no-code` subjects are similarly excluded:
/// they mark checkpoint reviews (gate tasks, empty squashes) that
/// produced no main commit by design. Ids with a matching
/// `state: forget-half-push bl-xxxx` commit are also excluded: that
/// marker is how SPEC §7.4 records an operator decision to retract a
/// stale warning (pre-0.3.8 gate reviews that predate the `no-code`
/// marker, recoveries handled out-of-band, etc.).
pub fn detect_half_push(store: &Store) -> Result<Vec<String>> {
    let state_dir = store.state_repo_dir();
    let state_subjects = git_state::log_subjects(&state_dir, "balls/tasks")?;
    let reviewed: std::collections::HashSet<String> = state_subjects
        .iter()
        .filter_map(|s| delivered_review_id(s))
        .collect();
    let forgotten: std::collections::HashSet<String> = state_subjects
        .iter()
        .filter_map(|s| extract_state_id(s, "state: forget-half-push "))
        .collect();
    // Per-task deliveries (bl-d4b0) record their effective integration
    // branch as a `target=<branch>` marker on the review subject; the
    // `[bl-xxxx]` tag for those lives on that branch, not the repo
    // level one. Newest review wins (state log is newest-first) so a
    // re-review with a changed target is scanned on its latest branch.
    // Absent marker ⇒ repo-level fallback, so a repo with no per-task
    // overrides scans exactly one branch as before (byte-identical).
    let mut targets: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for s in &state_subjects {
        if let Some((id, branch)) = reviewed_target(s) {
            targets.entry(id).or_insert(branch);
        }
    }
    let repo_main = store.load_config()?.integration_branch(&store.root)?;
    let repo_main_subjects = git_state::log_subjects(&store.root, &repo_main)?;
    let mut extra: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut missing = Vec::new();
    for subj in &state_subjects {
        let Some(id) = extract_state_id(subj, "state: close ") else { continue };
        if !reviewed.contains(&id) { continue; }
        if forgotten.contains(&id) { continue; }
        let tag = format!("[{id}]");
        let delivered = match targets.get(&id) {
            Some(b) if *b != repo_main => extra
                .entry(b.clone())
                .or_insert_with(|| git_state::log_subjects(&store.root, b).unwrap_or_default())
                .iter()
                .any(|s| s.contains(&tag)),
            _ => repo_main_subjects.iter().any(|s| s.contains(&tag)),
        };
        if !delivered && !missing.contains(&id) {
            missing.push(id);
        }
    }
    Ok(missing)
}

/// Extract `(id, branch)` from a `state: review <id> target=<branch>`
/// subject (bl-f788). The marker is emitted only when the task
/// delivered to a branch other than the repo-level integration branch,
/// so a subject without it (every pre-bl-f788 commit, every default
/// delivery) yields `None` and the caller falls back to the repo-level
/// branch — today's single-branch behavior. `no-code` reviews never
/// carry a target (nothing was delivered) and so also yield `None`.
pub(super) fn reviewed_target(subject: &str) -> Option<(String, String)> {
    let rest = subject.strip_prefix("state: review ")?;
    let mut parts = rest.split_whitespace();
    let id = parts.next()?;
    if !id.starts_with("bl-") {
        return None;
    }
    let branch = parts.find_map(|t| t.strip_prefix("target="))?;
    (!branch.is_empty()).then(|| (id.to_string(), branch.to_string()))
}

/// Write a `state: forget-half-push <id>` marker commit on the state
/// branch for each id. Per SPEC §7.4, retracting a stale half-push
/// warning is a recovery action that must leave a git-visible
/// record — hence an empty commit rather than a config-file entry.
/// Caller is responsible for ensuring the store is git-backed and
/// non-stealth (`cmd_repair` does this).
pub fn write_forget_half_push(store: &Store, ids: &[String]) -> Result<()> {
    let dir = store.state_repo_dir();
    for id in ids {
        let msg = format!("state: forget-half-push {id}");
        git::git_commit_empty(&dir, &msg)?;
    }
    Ok(())
}

pub(super) fn extract_state_id(subject: &str, prefix: &str) -> Option<String> {
    let rest = subject.strip_prefix(prefix)?;
    let id = rest.split_whitespace().next()?;
    id.starts_with("bl-").then(|| id.to_string())
}

/// Extract the task id from a `state: review bl-xxxx` subject iff the
/// review actually produced a main commit. A trailing `no-code` marker
/// means the review was a checkpoint (empty squash) — it should not
/// count as "reviewed" for half-push purposes.
fn delivered_review_id(subject: &str) -> Option<String> {
    let rest = subject.strip_prefix("state: review ")?;
    let mut parts = rest.split_whitespace();
    let id = parts.next()?;
    if !id.starts_with("bl-") || parts.next() == Some("no-code") {
        return None;
    }
    Some(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::{delivered_review_id, extract_state_id, reviewed_target};

    #[test]
    fn reviewed_target_extracts_marked_branch() {
        assert_eq!(
            reviewed_target("state: review bl-1234 target=develop"),
            Some(("bl-1234".into(), "develop".into()))
        );
    }

    #[test]
    fn reviewed_target_none_without_marker() {
        assert!(reviewed_target("state: review bl-1234").is_none());
        assert!(reviewed_target("state: review bl-1234 no-code").is_none());
        assert!(reviewed_target("state: close bl-1234").is_none());
        assert!(reviewed_target("state: review custom target=x").is_none());
        assert!(reviewed_target("state: review ").is_none());
        assert!(reviewed_target("state: review bl-1234 target=").is_none());
    }

    #[test]
    fn extract_state_id_handles_matching_prefix() {
        assert_eq!(
            extract_state_id("state: close bl-abcd - title", "state: close "),
            Some("bl-abcd".into())
        );
        assert_eq!(
            extract_state_id("state: review bl-1234", "state: review "),
            Some("bl-1234".into())
        );
    }

    #[test]
    fn extract_state_id_rejects_wrong_prefix() {
        assert!(extract_state_id("unrelated commit", "state: close ").is_none());
    }

    #[test]
    fn extract_state_id_rejects_non_task_id() {
        assert!(extract_state_id("state: close custom foo", "state: close ").is_none());
    }

    #[test]
    fn extract_state_id_rejects_empty_tail() {
        assert!(extract_state_id("state: close ", "state: close ").is_none());
    }

    #[test]
    fn delivered_review_id_classifies_review_subjects() {
        assert_eq!(
            delivered_review_id("state: review bl-1234"),
            Some("bl-1234".into())
        );
        assert!(delivered_review_id("state: review bl-1234 no-code").is_none());
        assert!(delivered_review_id("state: close bl-1234").is_none());
        assert!(delivered_review_id("unrelated").is_none());
        assert!(delivered_review_id("state: review custom").is_none());
        assert!(delivered_review_id("state: review ").is_none());
    }

    #[test]
    fn extract_state_id_parses_forget_half_push_subject() {
        assert_eq!(
            extract_state_id("state: forget-half-push bl-abcd", "state: forget-half-push "),
            Some("bl-abcd".into())
        );
        assert!(
            extract_state_id("state: close bl-abcd - t", "state: forget-half-push ").is_none()
        );
        assert_eq!(
            extract_state_id("state: forget-half-push bl-abcd stale", "state: forget-half-push "),
            Some("bl-abcd".into())
        );
    }
}
