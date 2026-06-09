//! Recency resolution over `balls/tasks` history (§2/§9, § id generation) — the
//! ONE walk every dead-ball lookup shares. A closed/dropped ball deletes its
//! `tasks/<id>.md` (§2, no archive dir); the deletion is not a tombstone but
//! older CONTENT, recoverable most-recent-down from `git log`.
//!
//! Both `bl show <id>` (a live miss) and `bl list --status closed/--all` reach the dead
//! set through here, so the discipline is factored once: for one id, find the
//! NEWEST commit that deleted it, reconstruct its frontmatter from that
//! deletion's PARENT (the last tree that still held the file), and derive the
//! retirement and its date from the deletion commit itself (§5).
//! Taking the newest deletion makes a reused id unambiguous — at most one
//! incarnation is ever live, so the most recent dead one is "the" dead ball.

use std::collections::HashSet;
use std::io;
use std::path::Path;

use super::Catalog;
use crate::git;
use crate::task::Task;
use crate::taskfile::invalid;

/// A ball reconstructed from history: its id, the frontmatter+body as it stood
/// the instant before deletion, and its deletion-commit date — the one fact the
/// gone file cannot carry. The deleting op is *not* reconstructed: every
/// retirement — a `close`, or a legacy `drop` deletion from before the verb
/// was deleted — projects as `closed`; the op stays git bedrock alone (§5).
pub(crate) struct Dead {
    pub id: String,
    pub task: Task,
    pub retired_at: i64,
}

/// The `\x1f` field separator the reconstruction `git log` format uses — a
/// control byte that cannot appear in a sha or unix timestamp, so the two
/// fields split unambiguously.
const SEP: char = '\u{1f}';

/// Reconstruct one dead ball by id, or `None` when `tasks/<id>.md` was never
/// deleted on this branch (so the id names nothing — live OR dead). The recency
/// walk's single id→content step: the caller checks the LIVE set first (§9).
pub(crate) fn resolve_dead(store: &Path, id: &str) -> io::Result<Option<Dead>> {
    let path = format!("tasks/{id}.md");
    let fmt = format!("--format=%H{SEP}%ct");
    // The newest commit that DELETED the file — its parent still held it.
    let log = git::run(store, &["log", "-1", "--diff-filter=D", &fmt, "--", &path], None)?;
    let log = log.trim_end_matches('\n');
    if log.is_empty() {
        return Ok(None); // no deletion in history ⇒ no dead incarnation
    }
    // git's format guarantees the separator, so the two fields are total.
    let mut fields = log.splitn(2, SEP);
    let sha = fields.next().expect("splitn always yields a first field");
    let ct = fields.next().expect("git --format emitted a %ct field");
    let retired_at = ct.parse().expect("git %ct is an integer unix timestamp");
    let content = git::run(store, &["show", &format!("{sha}^:{path}")], None)?;
    let task = Task::parse(&content).map_err(|e| invalid(e.to_string()))?;
    Ok(Some(Dead { id: id.to_string(), task, retired_at }))
}

/// Every currently-dead ball, newest-deletion first — the `list --status closed/--all`
/// set (§9). Enumerates each id ever deleted on `balls/tasks`, drops the ones
/// live again (a reused id resolves live, §9), and reconstructs the rest through
/// the same [`resolve_dead`] walk so there is one reconstruction path.
pub(crate) fn dead_balls(store: &Path, live: &Catalog) -> io::Result<Vec<Dead>> {
    let log = git::run(store, &["log", "--diff-filter=D", "--format=", "--name-only", "--", "tasks"], None)?;
    // `--format=` + `--name-only` prints one `tasks/<id>.md` per deleted file,
    // newest deletion first; the id strip silently skips any non-task path.
    let ids = log.lines().filter_map(|l| l.strip_prefix("tasks/").and_then(|f| f.strip_suffix(".md")));
    let mut seen = HashSet::new();
    let mut dead = Vec::new();
    for id in ids {
        // First sighting only (newest deletion); skip ids that are live again.
        if !seen.insert(id) || !live.is_resolved(id) {
            continue;
        }
        if let Some(d) = resolve_dead(store, id)? {
            dead.push(d);
        }
    }
    Ok(dead)
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
