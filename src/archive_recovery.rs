//! Recover closed/archived tasks from the state branch's git history.
//!
//! `Store::close_and_archive` `git rm`s a task's JSON from the
//! `balls/tasks` orphan branch, so a closed task survives only in that
//! branch's *history*. This module is the single engine both
//! `bl show <id>` (not-found fallback) and `bl list --closed/--all`
//! use to reconstruct those tasks. It is read-only git plumbing and
//! never mutates repository state.
//!
//! Subtlety worth stating once: `close_and_archive` removes the
//! *last-committed* blob without re-saving the in-memory closed task,
//! so the recovered JSON is the task in its **pre-close (review)**
//! state. The authoritative close facts live in the deletion commit,
//! not the file — we overlay `status = Closed` and `closed_at` from
//! that commit's timestamp. We deliberately fabricate nothing else
//! (no synthetic notes): `--json` stays a faithful view of what git
//! actually holds, and `bl show`'s `delivered:` line still resolves
//! the squash on main via the permanent `[bl-id]` tag.

use crate::git::clean_git_command;
use crate::git_state::STATE_BRANCH;
use crate::store::Store;
use crate::task::{Status, Task};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::Path;

/// True when archived-task recovery is even possible. Stealth and
/// no-git stores have no state branch, so closed tasks are gone for
/// good — callers surface a notice rather than silently returning
/// nothing.
pub fn available(store: &Store) -> bool {
    !store.stealth && !store.no_git
}

/// Run a git command in `dir`, returning trimmed-of-nothing stdout on
/// success. Best-effort: a spawn failure or non-zero exit maps to
/// `None`, because recovery is a read-side convenience that must never
/// abort the caller.
fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    let out = clean_git_command(dir).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn task_rel_path(id: &str) -> String {
    format!(".balls/tasks/{id}.json")
}

/// Parse a `<sha>\t<rfc3339>` pair into its components.
fn split_sha_date(line: &str) -> Option<(String, DateTime<Utc>)> {
    let (sha, date) = line.trim().split_once('\t')?;
    let when = DateTime::parse_from_rfc3339(date.trim())
        .ok()?
        .with_timezone(&Utc);
    Some((sha.to_string(), when))
}

/// Extract `bl-xxxx` from a `.balls/tasks/bl-xxxx.json` path line.
/// Notes sidecars (`*.notes.jsonl`) and scaffolding are ignored.
fn id_from_path(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix(".balls/tasks/")?
        .strip_suffix(".json")
        .filter(|id| id.starts_with("bl-"))
}

/// The most recent commit on the state branch that *deleted* this
/// task's JSON, with that commit's timestamp. Most-recent wins so a
/// recreate-then-reclose of the same id resolves to the latest close.
fn deletion_commit(dir: &Path, id: &str) -> Option<(String, DateTime<Utc>)> {
    let out = git_out(
        dir,
        &[
            "log",
            "-1",
            "--diff-filter=D",
            "--format=%H%x09%cI",
            STATE_BRANCH,
            "--",
            &task_rel_path(id),
        ],
    )
    .unwrap_or_default();
    split_sha_date(out.lines().next()?)
}

/// Reconstruct a `Task` from the blob just before `del_sha` removed
/// it, overlaying the authoritative close facts.
fn task_at_predeletion(
    dir: &Path,
    id: &str,
    del_sha: &str,
    closed_at: DateTime<Utc>,
) -> Option<Task> {
    let blob = git_out(dir, &["show", &format!("{del_sha}^:{}", task_rel_path(id))])?;
    let mut task: Task = serde_json::from_str(&blob).ok()?;
    task.status = Status::Closed;
    task.closed_at = Some(closed_at);
    Some(task)
}

/// Pure: fold `git log --name-only --format=%x01%H\t%cI` output into
/// the authoritative (most-recent) deletion per id. Skips notes
/// sidecars, ids still live at HEAD (`is_live` true — recreated after
/// an earlier close), and any entry without a parseable commit
/// header. Returns `(id, del_sha, closed_at)` sorted oldest close
/// first, id-tiebroken.
fn parse_deletions(
    log: &str,
    is_live: impl Fn(&str) -> bool,
) -> Vec<(String, String, DateTime<Utc>)> {
    let mut cur: Option<(String, DateTime<Utc>)> = None;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<(String, String, DateTime<Utc>)> = Vec::new();
    for line in log.lines() {
        if let Some(rest) = line.strip_prefix('\u{1}') {
            cur = split_sha_date(rest);
            continue;
        }
        let Some(id) = id_from_path(line) else { continue };
        // Log is newest-first: the first deletion seen for an id is
        // the authoritative (latest) close.
        if !seen.insert(id.to_string()) || is_live(id) {
            continue;
        }
        let Some((sha, when)) = &cur else { continue };
        out.push((id.to_string(), sha.clone(), *when));
    }
    out.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)));
    out
}

/// Recover a single closed task by id, or `None` if it was never
/// archived here (or recovery is unavailable in this store).
pub fn recover_one(store: &Store, id: &str) -> Option<Task> {
    if !available(store) {
        return None;
    }
    let dir = store.state_worktree_dir();
    let (sha, closed_at) = deletion_commit(&dir, id)?;
    task_at_predeletion(&dir, id, &sha, closed_at)
}

/// Recover every archived task, reconstructed from the state branch's
/// deletion history, oldest close first. Empty when recovery is
/// unavailable or nothing was ever closed.
pub fn recover_all(store: &Store) -> Vec<Task> {
    if !available(store) {
        return Vec::new();
    }
    let dir = store.state_worktree_dir();
    let log = git_out(
        &dir,
        &[
            "log",
            "--diff-filter=D",
            "--name-only",
            "--format=format:%x01%H%x09%cI",
            STATE_BRANCH,
            "--",
            ".balls/tasks/",
        ],
    )
    .unwrap_or_default();
    parse_deletions(&log, |id| store.task_exists(id))
        .into_iter()
        .filter_map(|(id, sha, when)| task_at_predeletion(&dir, &id, &sha, when))
        .collect()
}

#[cfg(test)]
#[path = "archive_recovery_tests.rs"]
mod tests;
