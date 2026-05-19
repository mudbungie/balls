//! Generous anti-DoS backstops for plugin sync ingest.
//!
//! Bidirectional plugin sync makes every SyncCreate/SyncUpdate field
//! attacker-influenced and lands it as a committed file on
//! `balls/tasks`. These are *backstops*, not content policy: every
//! ceiling here sits orders of magnitude above any real GitHub/Jira
//! payload, every one is env-overridable, and the failure mode is
//! always truncate-or-skip-and-warn — never reject the sync. Big real
//! data (long descriptions, many tags, fat external blobs) passes
//! through byte-unchanged. The only thing defended against is
//! pathological abuse: an unbounded field or a flood of creates that
//! would OOM the process or wedge the repo. The whole-stream memory
//! ceiling lives one layer down in `plugin::limits`
//! (`effective_stream_cap`); this module is the per-item layer.

use std::fmt::Write as _;

/// Flood backstop on tasks created in one sync. Real trackers never
/// report this many *new* issues at once; a plugin that does is
/// flooding. Warn-not-fail: the excess is skipped, the rest applies.
const DEFAULT_MAX_SYNC_CREATES: usize = 5_000;

/// Per-text-field byte ceiling. 1 MiB is absurd for a title or
/// description (real ones are bytes-to-kilobytes); a field past it is
/// truncated with a visible marker, never dropped, never rejected.
const DEFAULT_MAX_FIELD_BYTES: usize = 1024 * 1024;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Max tasks created from one plugin sync
/// (`BALLS_PLUGIN_MAX_SYNC_CREATES`).
pub fn max_sync_creates() -> usize {
    env_usize("BALLS_PLUGIN_MAX_SYNC_CREATES", DEFAULT_MAX_SYNC_CREATES)
}

/// Per-text-field byte ceiling
/// (`BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES`).
pub fn max_field_bytes() -> usize {
    env_usize("BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES", DEFAULT_MAX_FIELD_BYTES)
}

/// Split `created` at `cap`: the prefix to apply, and how many were
/// dropped (0 in every real case). Pure in `cap` so the policy is
/// unit-testable without touching the environment.
fn clamp_to<T>(created: &[T], cap: usize) -> (&[T], usize) {
    if created.len() <= cap {
        (created, 0)
    } else {
        (&created[..cap], created.len() - cap)
    }
}

/// Apply the configured flood ceiling. See [`clamp_to`].
pub fn clamp_creates<T>(created: &[T]) -> (&[T], usize) {
    clamp_to(created, max_sync_creates())
}

/// Truncate `s` to at most `cap` bytes on a char boundary, appending
/// a visible marker that records the original size. No-op when within
/// budget. Returns the original byte length iff it truncated, so the
/// caller can emit a diagnostic. Pure in `cap` for unit testing.
fn truncate_to(s: &mut String, cap: usize) -> Option<usize> {
    if s.len() <= cap {
        return None;
    }
    let orig = s.len();
    let mut end = cap;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    // `write!` into a String is infallible; clippy prefers it over
    // `push_str(&format!(..))` to skip the throwaway allocation.
    let _ = write!(
        s,
        " […balls truncated this field: {orig} bytes over the \
         {cap}-byte sync-ingest backstop; raise \
         BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES if this was a real payload]"
    );
    Some(orig)
}

/// Apply the configured per-field ceiling. See [`truncate_to`].
pub fn truncate_field(s: &mut String) -> Option<usize> {
    truncate_to(s, max_field_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_under_and_at_cap_is_passthrough() {
        let v = [1, 2, 3];
        assert_eq!(clamp_to(&v, 5), (&v[..], 0));
        assert_eq!(clamp_to(&v, 3), (&v[..], 0));
    }

    #[test]
    fn clamp_over_cap_keeps_prefix_and_reports_dropped() {
        let v = [1, 2, 3, 4, 5];
        let (kept, dropped) = clamp_to(&v, 2);
        assert_eq!(kept, &[1, 2]);
        assert_eq!(dropped, 3);
    }

    #[test]
    fn truncate_within_budget_is_noop() {
        let mut s = "small".to_string();
        assert_eq!(truncate_to(&mut s, 1024), None);
        assert_eq!(s, "small");
    }

    #[test]
    fn truncate_on_ascii_boundary_keeps_prefix_and_marks() {
        let mut s = "A".repeat(100);
        let orig = truncate_to(&mut s, 10);
        assert_eq!(orig, Some(100));
        assert!(s.starts_with("AAAAAAAAAA"));
        assert!(s.contains("balls truncated this field: 100 bytes"));
    }

    #[test]
    fn truncate_backtracks_off_a_multibyte_char() {
        // 'é' is 2 bytes (4..6); cap 5 lands mid-char, so the
        // boundary loop must back up to byte 4 ("test") before
        // truncating. The marker still reports the true original len.
        let s0 = "testé more text here padding padding".to_string();
        let mut s = s0.clone();
        let orig = truncate_to(&mut s, 5).unwrap();
        assert_eq!(orig, s0.len());
        assert!(s.starts_with("test"));
        assert!(!s.starts_with("testé"));
        assert!(s.contains("balls truncated this field"));
    }
}
