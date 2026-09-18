//! §13 drift render (bl-3616 §6 Q5, bl-439d) — "is this store published?",
//! detected at every op in the op's scope, reconciled by none of them.
//!
//! Three surfaces, ONE derivation, nothing stored on any ball:
//! - every mutating op's `*.post`, on stderr (the confirmation channel), for
//!   the op's own ball: `bl-xxxx: 2 seals unpublished` ([`op_line`]) — silent
//!   at zero, which is what a mandatory push leaves behind;
//! - `bl show <id>`: a per-ball `published` field line ([`show_line`]);
//! - `bl list`: the store-level header ([`list_header`]) — list gets
//!   everything, so it gets the aggregate, not a column.
//!
//! The derivation is `git rev-list --count` between the store branch and the
//! tracker's PUBLICATION MARK: the last remote tip this store positively knew
//! — set to `FETCH_HEAD` after every fetch and to `HEAD` after every
//! successful push ([`mark`]). Git's own remote-tracking refs would be that
//! mark, but the store remote is a URL (§12 ladder), which git tracks nowhere,
//! so the tracker keeps the one ref itself: a file in the store's gitdir
//! (territory bl owns — the seen-token precedent, `crate::seen`), sha as
//! content, mtime as the fetch age. *Ahead* is therefore free and exact;
//! *behind* is only as fresh as the last fetch, and says so — §12's refusal of
//! a per-op pre-pull stands, so detection is honest about its own staleness
//! rather than paying a round-trip to hide it. Never in `--json` (a read
//! dispatch never runs there); absent in stealth (no remote ⇒ no line).

use super::git::git;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The publication mark's file name inside the store's gitdir.
const MARK: &str = "balls-published";

/// Record `sha` as the last remote tip this store positively knew. Best-effort
/// by design (a lost mark costs one stale render, never correctness).
pub(super) fn mark(store: &Path, sha: &str) {
    if let Ok(path) = mark_path(store) {
        let _ = fs::write(path, format!("{sha}\n"));
    }
}

/// `<store gitdir>/balls-published` — the per-worktree gitdir, so the landing
/// (a sibling worktree of the same repo) never sees the store's mark.
fn mark_path(store: &Path) -> io::Result<PathBuf> {
    let dir = git(store, &["rev-parse", "--git-dir"])?;
    Ok(store.join(dir).join(MARK))
}

/// The derived drift of the store (or of one ball's file when `id` is given)
/// against the mark: seals here the mark lacks (`ahead`), seals at the mark
/// this branch lacks (`behind`), and when the mark was last set (`fetched`,
/// unix seconds). `None` when no mark exists — nothing has ever been fetched or
/// pushed here, so there is nothing to compare against.
pub(super) struct Drift {
    pub ahead: u64,
    pub behind: u64,
    pub fetched: i64,
}

pub(super) fn read(store: &Path, id: Option<&str>) -> Option<Drift> {
    let path = mark_path(store).ok()?;
    let sha = fs::read_to_string(&path).ok()?.trim().to_string();
    let fetched = fs::metadata(&path).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let count = |range: String| -> Option<u64> {
        let mut args = vec!["rev-list", "--count", &range];
        let file = id.map(|id| format!("tasks/{id}.md"));
        if let Some(f) = &file {
            args.extend(["--", f]);
        }
        git(store, &args).ok()?.parse().ok()
    };
    let fetched = i64::try_from(fetched).unwrap_or(i64::MAX);
    let drift = Drift { ahead: count(format!("{sha}..HEAD"))?, behind: count(format!("HEAD..{sha}"))?, fetched };
    Some(drift)
}

/// `N seal(s)`, pluralized once.
fn seals(n: u64) -> String {
    format!("{n} seal{}", if n == 1 { "" } else { "s" })
}

/// `(last fetch <age> ago)` — the staleness stamp every behind-reading carries.
fn stamp(fetched: i64, now: i64) -> String {
    format!("(last fetch {} ago)", crate::reads::claim_age::humanize(now - fetched))
}

/// The `*.post` stderr line for the op's ball (`store` when the op names none —
/// the bulk `import`): `Some` only when something is unpublished, so a
/// mandatory push leaves the channel quiet and the line means what it says.
pub(super) fn op_line(store: &Path, id: Option<&str>) -> Option<String> {
    let drift = read(store, id)?;
    (drift.ahead > 0).then(|| format!("{}: {} unpublished", id.unwrap_or("store"), seals(drift.ahead)))
}

/// The `bl show` field line: `published  <state> (last fetch <age> ago)`.
pub(super) fn show_line(store: &Path, id: &str, now: i64) -> String {
    let body = match read(store, Some(id)) {
        None => "unknown — never synced with the remote (bl sync)".to_string(),
        Some(d) => format!("{} {}", state(&d), stamp(d.fetched, now)),
    };
    // `published` is nine wide — one past show's `{:<9}` label column — so the
    // value sits one column right of its siblings rather than jammed on.
    format!("  published {body}\n")
}

/// The per-ball state word(s): what is unpublished here, what is newer there.
fn state(d: &Drift) -> String {
    match (d.ahead, d.behind) {
        (0, 0) => "current".to_string(),
        (a, 0) => format!("{} unpublished", seals(a)),
        (0, b) => format!("{} to sync", seals(b)),
        (a, b) => format!("{} unpublished, {} to sync", seals(a), seals(b)),
    }
}

/// The `bl list` header: the store-level aggregate against the remote.
pub(super) fn list_header(store: &Path, remote: &str, branch: &str, now: i64) -> String {
    match read(store, None) {
        None => format!("store: publication unknown — never synced with `{remote}` {branch} (bl sync)\n"),
        Some(d) => format!("store: {} ahead, {} behind `{remote}` {branch} {}\n", d.ahead, d.behind, stamp(d.fetched, now)),
    }
}

/// The read-op dispatch (`show`/`list`, single phase `read`, §6): fold the
/// drift line onto `out`, the plugin's captured stdout. Stealth (no remote)
/// prints nothing — there is no publication to be behind on.
pub(super) fn render(op: &str, b: &super::Binding, id: Option<&str>, out: &mut impl Write) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    let store = Path::new(&b.store);
    let now = crate::log::wall();
    match (op, id) {
        ("show", Some(id)) => write!(out, "{}", show_line(store, id, now)),
        ("list", _) => write!(out, "{}", list_header(store, remote, &b.tasks_branch, now)),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "drift_tests.rs"]
mod tests;
