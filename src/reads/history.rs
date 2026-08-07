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
//!
//! **The plural read is BATCHED, and that is the whole performance story
//! (bl-4c08).** Singular and plural share the discipline, not the plumbing: one
//! id costs one `git log`, but N ids must not cost N of them. A per-id
//! `git log -1 -- tasks/<id>.md` walks from HEAD until it reaches that ball's
//! deletion, so its cost grows with the ball's AGE, and history length grows
//! with ball count — N such walks is quadratic. [`dead_balls`] instead pays ONE
//! walk (which already names every deletion's sha) and ONE `cat-file --batch`
//! for every pre-deletion blob: O(history + dead), two subprocesses, whatever N
//! is. Measured on this repo's own store (395 dead over 1193 commits): 7.4s →
//! 0.087s. Nothing is stored, cached, or indexed to get that — the redundant
//! re-derivation was simply deleted (§0 derive-don't-store is untouched; there
//! is no second representation to drift).

use std::collections::HashSet;
use std::fmt::Write;
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
    /// The revision whose tree still holds `tasks/<id>.md` — the deletion's
    /// PARENT, the coordinate this reconstruction was read from. Carried rather
    /// than re-derived: the walk below already knows it, and a content-derived
    /// read of the same bytes (the §9 comment byline's blame, bl-236c) must
    /// address the very revision the rendered body came from.
    pub rev: String,
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
    let rev = format!("{sha}^");
    let content = git::run(store, &["show", &format!("{rev}:{path}")], None)?;
    let task = Task::parse(&content).map_err(|e| invalid(e.to_string()))?;
    Ok(Some(Dead { id: id.to_string(), task, retired_at, rev }))
}

/// One enumerated deletion, before its content is read: the ball's id, the
/// `<sha>^` revision still holding its last live bytes, and the deletion date.
/// The enumeration walk already knows all three, so nothing here is re-derived
/// per id — the object name the batch reads is that revision plus the path.
struct Deletion {
    id: String,
    rev: String,
    retired_at: i64,
}

/// Every currently-dead ball, newest-deletion first — the `list --status closed/--all`
/// set (§9). Two subprocesses whatever the store's size: [`newest_deletions`]
/// enumerates, then ONE `cat-file --batch` resolves every pre-deletion blob in
/// the order the enumeration fixed, so the reply stream and the deletion list
/// zip positionally.
///
/// This does NOT go through [`resolve_dead`] — that is the point (see the module
/// header). The two paths still share the one reconstruction DISCIPLINE (newest
/// deletion wins, content from the deletion's parent, date from the deletion
/// commit); what they no longer share is a per-id `git log`.
pub(crate) fn dead_balls(store: &Path, live: &Catalog) -> io::Result<Vec<Dead>> {
    let deletions = newest_deletions(store, live)?;
    // One newline-terminated object name per reply; an empty list is an empty
    // batch (git reads EOF and exits clean), so the no-dead-balls store needs no
    // special case.
    let mut names = String::new();
    for d in &deletions {
        let _ = writeln!(names, "{}:tasks/{}.md", d.rev, d.id);
    }
    let batch = git::run_bytes(store, &["cat-file", "--batch"], Some(&names))?;
    let mut stream = batch.as_slice();
    let mut dead = Vec::with_capacity(deletions.len());
    for d in deletions {
        let (content, rest) = next_object(stream);
        stream = rest;
        let task = Task::parse(&content).map_err(|e| invalid(e.to_string()))?;
        dead.push(Dead { id: d.id, task, retired_at: d.retired_at, rev: d.rev });
    }
    Ok(dead)
}

/// The newest deletion of each id ever deleted under `tasks/`, newest first,
/// with ids that are live again dropped (a reused id resolves live, §9).
///
/// `--format` + `--name-only` interleaves one `<sha>\x1f<ct>` header per commit
/// with the paths it deleted; a path line can never contain [`SEP`], so the two
/// kinds of line tell themselves apart and the header in force is the one that
/// deleted the paths under it.
fn newest_deletions(store: &Path, live: &Catalog) -> io::Result<Vec<Deletion>> {
    let fmt = format!("--format=%H{SEP}%ct");
    let log = git::run(store, &["log", "--diff-filter=D", &fmt, "--name-only", "--", "tasks"], None)?;
    let mut seen = HashSet::new();
    let mut deletions = Vec::new();
    let mut at = None;
    for line in log.lines() {
        if let Some((sha, ct)) = line.split_once(SEP) {
            at = Some((sha, ct.parse().expect("git %ct is an integer unix timestamp")));
        } else if let Some(id) = line.strip_prefix("tasks/").and_then(|f| f.strip_suffix(".md")) {
            // First sighting only (newest deletion); skip ids that are live again.
            if !seen.insert(id) || !live.is_resolved(id) {
                continue;
            }
            let (sha, retired_at) = at.expect("git log prints a commit header before the paths it touched");
            deletions.push(Deletion { id: id.to_string(), rev: format!("{sha}^"), retired_at });
        }
    }
    Ok(deletions)
}

/// Split one `cat-file --batch` reply off the front of `stream`: a
/// `<sha> <type> <size>` header line, then exactly `size` bytes of content, then
/// a newline. Returns the content and the rest of the stream.
///
/// Framed on BYTES, not chars: the size is a byte count, so decoding before
/// splitting would let one invalid byte (rewritten as a 3-byte `U+FFFD`) desync
/// every reply after it. Every object name fed to the batch came from git's own
/// deletion log, so `missing`/`ambiguous` replies — the only ones without a size
/// — cannot occur, and the framing is total.
fn next_object(stream: &[u8]) -> (String, &[u8]) {
    let nl = stream.iter().position(|b| *b == b'\n').expect("cat-file --batch emits one header line per request");
    let header = String::from_utf8_lossy(&stream[..nl]);
    let size: usize = header
        .rsplit(' ')
        .next()
        .and_then(|s| s.parse().ok())
        .expect("a cat-file --batch header ends in the object's byte size");
    let body = nl + 1;
    (String::from_utf8_lossy(&stream[body..body + size]).into_owned(), &stream[body + size + 1..])
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
