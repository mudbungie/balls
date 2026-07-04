//! Root-aware `bl list` scope + fleet-view project labels (bl-0161 Q2, bl-5965).
//!
//! `list`'s default set is the claim-admitted set: the SAME predicate the claim
//! guard enforces ([`crate::change::admits`]), over this checkout's root SET, so
//! `list` shows exactly what `claim` will take. `--everywhere` lifts the scope
//! and, on the HUMAN render only, shadows a foreign ball's root hash with an
//! enrolled checkout's directory basename (short-hash fallback).
//!
//! Both git reads are LAZY. The root read is skipped entirely when no listed
//! ball carries a root ([`checkout_roots`]'s `needed` gate — a task-only store
//! stays walk-free) and paid at most once per invocation. The enrolled-checkout
//! enumeration ([`enrolled_labels`]) runs only under `--everywhere` and only when
//! a foreign row actually exists. Nothing is cached: the root walk is
//! once-per-command, never stored (a durable cache would drift at exactly the
//! history-rewrite moment it matters, bl-0161).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::change::admits;
use crate::delivery_repo::Project;
use crate::encoding::percent_decode;
use crate::layout::Xdg;

/// This checkout's root SET (`git rev-list --max-parents=0 HEAD`, newest-first),
/// computed only when `needed` — some listed ball carries a root, so the answer
/// can change the set. A rootless-only catalog passes `needed = false` and never
/// shells git; a non-git checkout yields an empty set, which [`admits`] fails open
/// on (a rootless checkout sees everything). The one full-history walk a listing
/// pays, and only here.
#[must_use]
pub(crate) fn checkout_roots(invocation_path: &Path, needed: bool) -> Vec<String> {
    if needed {
        Project::at(invocation_path).root_commits()
    } else {
        Vec::new()
    }
}

/// Is a ball with `root_commit` FOREIGN to this checkout — a real recorded root
/// that [`admits`] refuses here? Rootless balls (`None`) are admitted everywhere
/// (fail-open), so never foreign; under `--everywhere` a foreign row is the one
/// that earns a project label.
#[must_use]
pub(crate) fn is_foreign(root_commit: Option<&str>, this_roots: &[String]) -> bool {
    root_commit.is_some() && !admits(root_commit, this_roots)
}

/// The human fleet-view label suffix for one row: `  [<label>]` when the row is
/// foreign AND labels are in play (`--everywhere`), `""` otherwise. The label is
/// an enrolled checkout's basename when one is rooted at the ball's root, else the
/// short hash ([`Labels::of`]). Render-time only — never enters `--json`.
#[must_use]
pub(crate) fn row_label(labels: Option<&Labels>, root_commit: Option<&str>, this_roots: &[String]) -> String {
    let (Some(labels), Some(root)) = (labels, root_commit) else { return String::new() };
    if is_foreign(Some(root), this_roots) {
        format!("  [{}]", labels.of(root))
    } else {
        String::new()
    }
}

/// The root-hash → human-label map behind the fleet view — built once per
/// `--everywhere` render that has a foreign row, never stored.
pub(crate) struct Labels {
    by_root: HashMap<String, String>,
}

impl Labels {
    /// The label for a foreign `root`: an enrolled checkout's directory basename
    /// when one on this box is rooted at it, else the short (8-char) hash — a
    /// name never appears where the box can't earn it (bl-0161).
    #[must_use]
    fn of(&self, root: &str) -> String {
        self.by_root.get(root).cloned().unwrap_or_else(|| short(root))
    }
}

/// The short (8-char) prefix of a root hash — the fleet-view fallback when no
/// enrolled checkout on this box is rooted at it.
fn short(root: &str) -> String {
    root.chars().take(8).collect()
}

/// Map each enrolled checkout's root(s) → its directory basename, for the human
/// fleet view. Enumerate `clones/<pct-enc-path>/` (bl-5965), decode each entry to
/// its checkout path, skip any that no longer exists, compute its root set, and
/// register the basename for every root (first writer wins — an arbitrary but
/// stable pick when two checkouts share a root). Config-free: the names come from
/// the box's OWN primed checkouts, never a declared roster. A broken or foreign
/// entry is skipped silently — a label degrades to the short hash, never an error.
#[must_use]
pub(crate) fn enrolled_labels(xdg: &Xdg) -> Labels {
    let mut by_root = HashMap::new();
    let Ok(entries) = fs::read_dir(xdg.clones_dir()) else {
        return Labels { by_root }; // no clones dir yet → nothing to name
    };
    for entry in entries.flatten() {
        let Some((path, basename)) = checkout_of(&entry.file_name()) else { continue };
        if !path.exists() {
            continue; // an enrolled checkout since removed — no name to earn
        }
        for root in Project::at(&path).root_commits() {
            by_root.entry(root).or_insert_with(|| basename.clone());
        }
    }
    Labels { by_root }
}

/// Recover an enrolled checkout's `(path, directory-basename)` from its
/// `clones/<pct-enc-path>/` entry name — `None` for a name the encoder never
/// wrote (fails to decode) or a path with no final component (e.g. `/`).
fn checkout_of(entry_name: &std::ffi::OsStr) -> Option<(PathBuf, String)> {
    let path = PathBuf::from(percent_decode(entry_name.to_str()?)?);
    let basename = path.file_name()?.to_string_lossy().into_owned();
    Some((path, basename))
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
