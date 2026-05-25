//! Per-clone path breadcrumb for moved-clone detection per
//! [SPEC-clone-layout.md] §8 + Phase 3 (bl-05e5).
//!
//! When `bl` materializes the per-clone tree under
//! `~/.local/state/balls/{claims,locks,worktrees,plugins-auth}/<nested>/`,
//! it drops a `clone-path.json` breadcrumb under `claims/<nested>/`
//! recording the clone's absolute filesystem path and the host that
//! wrote it. `bl doctor` reads these breadcrumbs to detect moved
//! clones: an orphaned per-clone subtree whose recorded path no longer
//! matches the current clone's path becomes an actionable finding with
//! a `bl repair --rebind-path` suggestion.
//!
//! Why `claims/`: it is the most stable per-clone artifact (locks come
//! and go; plugins-auth is sparse; worktrees only exist while
//! claimed). The breadcrumb sits at the root of `claims/<nested>/` so
//! the recursive walk in [`crate::doctor`] can find it by scanning the
//! `claims/` tree alone.
//!
//! Cross-host scoping: the breadcrumb records `$HOSTNAME` alongside
//! the path so doctor (running on host A) does not report a
//! per-clone subtree owned by host B on a shared `$HOME` as "moved."
//! Two hosts on shared NFS will each write their own breadcrumb when
//! they first materialize state; doctor filters by hostname.
//!
//! [SPEC-clone-layout.md]: ../docs/SPEC-clone-layout.md

use crate::encoding::nested_clone_path;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};

/// File name of the breadcrumb inside `claims/<nested>/`. Public so
/// the doctor walk can match it without recomputing the constant.
pub const BREADCRUMB_FILE: &str = "clone-path.json";

/// Recorded clone identity. `path` is the clone's absolute
/// filesystem path at the time of recording; `hostname` is the host
/// that wrote it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloneBreadcrumb {
    pub path: String,
    pub hostname: String,
}

impl CloneBreadcrumb {
    /// Build a breadcrumb for `clone_root` on the current host.
    /// `hostname()` is best-effort: a host with no `gethostname` (or
    /// a host returning garbage) records `"unknown"` — moved-clone
    /// detection then collapses to a path-only comparison on that
    /// host, which is exactly the SPEC §8 same-host default minus
    /// the cross-host guard.
    #[must_use]
    pub fn new(clone_root: &Path) -> Self {
        Self {
            path: clone_root.to_string_lossy().into_owned(),
            hostname: hostname(),
        }
    }
}

/// Best-effort hostname read via `libc::gethostname`. Falls back to
/// the `HOSTNAME` env var, then `"unknown"`. Pure read — never mutates
/// state, never errors. The env-var fallback covers hosts where
/// `gethostname` returns an empty or unterminated string; the
/// `"unknown"` tail covers a hostless test environment.
#[must_use]
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    // Safety: gethostname writes at most buf.len() bytes and
    // null-terminates on success.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    parse_gethostname(rc, &buf).unwrap_or_else(env_hostname)
}

/// Pure: turn a `gethostname` `(rc, buf)` pair into the recorded
/// hostname when the call succeeded with non-empty output. Pulled
/// out so the failure branches have direct unit coverage without
/// faking out `libc` itself (and without env mutation, which races
/// parallel tests per the bl-bfa8 incident).
#[must_use]
pub(crate) fn parse_gethostname(rc: libc::c_int, buf: &[u8]) -> Option<String> {
    if rc != 0 {
        return None;
    }
    let cstr = CStr::from_bytes_until_nul(buf).ok()?;
    let s = cstr.to_string_lossy().into_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Env-var hostname fallback: `HOSTNAME` if set, else literal
/// `"unknown"`. Pulled out so the literal-fallback branch has a
/// direct test without mutating the process-wide `HOSTNAME` (env
/// mutation races parallel tests; bl-bfa8/bl-ad4b).
#[must_use]
pub(crate) fn env_hostname() -> String {
    env_hostname_from(std::env::var("HOSTNAME").ok())
}

#[must_use]
pub(crate) fn env_hostname_from(env: Option<String>) -> String {
    env.unwrap_or_else(|| "unknown".into())
}

/// The breadcrumb's on-disk path under a per-clone `claims/<nested>/`
/// directory. The single source of truth so writer + reader agree
/// without coordinating string literals.
#[must_use]
pub fn breadcrumb_path(claims_dir: &Path) -> PathBuf {
    claims_dir.join(BREADCRUMB_FILE)
}

/// Write (or overwrite) the breadcrumb at `claims_dir/clone-path.json`.
/// The breadcrumb is overwritten on every call: an idempotent rewrite
/// keeps the recorded path canonical even when the clone has been
/// reachable by multiple paths (e.g. `/home/.../balls` vs a `realpath`
/// resolution of a symlinked checkout). Creates `claims_dir` if
/// missing so callers can use the bundle constructor's path directly.
pub fn write_at(claims_dir: &Path, clone_root: &Path) -> Result<()> {
    fs::create_dir_all(claims_dir)?;
    let bc = CloneBreadcrumb::new(clone_root);
    let json = serde_json::to_string_pretty(&bc)? + "\n";
    fs::write(breadcrumb_path(claims_dir), json)?;
    Ok(())
}

/// Read the breadcrumb if present. Returns `None` when the file is
/// absent or unreadable as JSON — a corrupt breadcrumb is treated the
/// same as a missing one (doctor would re-write it on the next prime
/// once Phase 1B's claim flip lands; for now back-fill via prime).
#[must_use]
pub fn read_at(claims_dir: &Path) -> Option<CloneBreadcrumb> {
    let s = fs::read_to_string(breadcrumb_path(claims_dir)).ok()?;
    serde_json::from_str(&s).ok()
}

/// Back-fill the breadcrumb when the per-clone tree exists but the
/// breadcrumb does not. Run from `bl prime` so a pre-bl-05e5 clone
/// (no breadcrumb at materialization) earns one on its first XDG
/// session. Silent no-op when `claims_dir` is absent — nothing to
/// back-fill on a clone that has never materialized per-clone state.
pub fn backfill(claims_dir: &Path, clone_root: &Path) -> Result<()> {
    if !claims_dir.exists() {
        return Ok(());
    }
    if breadcrumb_path(claims_dir).exists() {
        return Ok(());
    }
    write_at(claims_dir, clone_root)
}

/// Compute the `<nested-clone-path>` for `clone_root` per SPEC §4 — a
/// thin wrapper around [`crate::encoding::nested_clone_path`] kept in
/// this module so the doctor's moved-clone code can read its inputs
/// from one place.
#[must_use]
pub fn nested_for(clone_root: &Path) -> PathBuf {
    nested_clone_path(clone_root)
}

#[cfg(test)]
#[path = "clone_breadcrumb_tests.rs"]
mod tests;
