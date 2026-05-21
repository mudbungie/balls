//! Unit tests for `delivery`: local-resolve branches, the bl-7523
//! `populate_on_close` provenance writes, and the bl-f37b `resolve_with`
//! remote-fallback gate. Split out of `delivery.rs` to keep that file
//! under the 300-line cap; the bl-e454 `populate_on_close` remote-knob
//! tests live in `delivery_close_remote_tests.rs` for the same reason.
//! Fixtures are in `delivery_test_support`.

use super::*;
use crate::delivery_test_support::{empty_task, local_repo_with_tag};
use tempfile::TempDir;

#[test]
fn resolve_returns_empty_when_not_a_git_repo() {
    // Not a git repo: every git query fails, so resolve yields an
    // empty, non-stale result regardless of the branch passed.
    let dir = TempDir::new().unwrap();
    let d = resolve(dir.path(), "main", &empty_task());
    assert!(d.sha.is_none());
    assert!(!d.hint_stale);
    assert!(d.resolved_repo.is_none());
}

#[test]
fn populate_on_close_manual_override_wins_unconditionally() {
    // `bl close --delivered <sha>` skips the scan and sets the
    // hint even when one is already present (forge rebase-merge).
    // bl-7523: without `--delivered-repo`, the manual sha is by
    // definition local-resolvable, so `delivered_repo` auto-tags
    // with the current repo.
    let dir = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_in = Some("oldsha".into());
    let changed = populate_on_close(dir.path(), "main", &mut t, Some("forced".into()), None, false);
    assert!(changed);
    assert_eq!(t.delivered_in.as_deref(), Some("forced"));
    assert_eq!(
        t.delivered_repo.as_deref(),
        Some(crate::repo_url::current(dir.path()).as_str())
    );
}

#[test]
fn populate_on_close_is_noop_when_hint_already_set() {
    // Local-squash mode wrote the hint in `review`; close must
    // not touch it (no scan, byte-identical archived task). The
    // bl-7523 provenance the review path already wrote stays put.
    let dir = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_in = Some("fromreview".into());
    t.delivered_repo = Some("git@h:from-review.git".into());
    let changed = populate_on_close(dir.path(), "main", &mut t, None, None, false);
    assert!(!changed);
    assert_eq!(t.delivered_in.as_deref(), Some("fromreview"));
    assert_eq!(t.delivered_repo.as_deref(), Some("git@h:from-review.git"));
}

#[test]
fn populate_on_close_scan_miss_leaves_hint_null() {
    // Null hint, no `[id]` commit reachable (not a git repo, so
    // the tag scan finds nothing): warn and proceed with null —
    // no sha, no provenance.
    let dir = TempDir::new().unwrap();
    let mut t = empty_task();
    let changed = populate_on_close(dir.path(), "main", &mut t, None, None, false);
    assert!(!changed);
    assert!(t.delivered_in.is_none());
    assert!(t.delivered_repo.is_none());
}

#[test]
fn populate_on_close_local_hit_auto_tags_current_repo() {
    // bl-6816 regression: a null-hint close that resolves the sha via
    // the *local* tag scan still auto-tags `delivered_repo` with the
    // closing clone — `resolved_repo` equals `repo_url::current` on a
    // local hit, so threading it through is byte-identical to the
    // pre-bl-6816 behavior. Only the remote-hit path changes.
    let (dir, sha) = local_repo_with_tag("bl-abcd");
    let mut t = empty_task();
    let changed = populate_on_close(dir.path(), "main", &mut t, None, None, false);
    assert!(changed);
    assert_eq!(t.delivered_in.as_deref(), Some(sha.as_str()));
    assert_eq!(
        t.delivered_repo.as_deref(),
        Some(crate::repo_url::current(dir.path()).as_str())
    );
}

#[test]
fn populate_on_close_manual_repo_overrides_auto_tag() {
    // bl-733e: `--delivered <sha> --delivered-repo <url>` writes
    // both fields verbatim — the operator's declared source wins
    // over the local clone's `origin` auto-tag.
    let dir = TempDir::new().unwrap();
    let mut t = empty_task();
    let changed = populate_on_close(
        dir.path(),
        "main",
        &mut t,
        Some("forced".into()),
        Some("git@h:client-a.git".into()),
        false,
    );
    assert!(changed);
    assert_eq!(t.delivered_in.as_deref(), Some("forced"));
    assert_eq!(t.delivered_repo.as_deref(), Some("git@h:client-a.git"));
}

#[test]
fn populate_on_close_manual_repo_alone_updates_only_provenance() {
    // bl-733e: `--delivered-repo <url>` without `--delivered`
    // corrects the source repo on a task that already has a sha
    // (typical bridge-clone sync hook case). delivered_in stays
    // untouched; delivered_repo gets the new value.
    let dir = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_in = Some("fromreview".into());
    t.delivered_repo = Some("git@h:wrong.git".into());
    let changed = populate_on_close(
        dir.path(),
        "main",
        &mut t,
        None,
        Some("git@h:right.git".into()),
        false,
    );
    assert!(changed);
    assert_eq!(t.delivered_in.as_deref(), Some("fromreview"));
    assert_eq!(t.delivered_repo.as_deref(), Some("git@h:right.git"));
}

#[test]
fn populate_on_close_manual_repo_writes_even_when_no_sha_resolves() {
    // bl-733e: declaring a source repo on a task with no sha
    // (scan miss, no manual sha) is allowed — the operator opted
    // in explicitly. delivered_in stays null; we still return
    // true so the caller persists the provenance.
    let dir = TempDir::new().unwrap();
    let mut t = empty_task();
    let changed = populate_on_close(
        dir.path(),
        "main",
        &mut t,
        None,
        Some("git@h:c.git".into()),
        false,
    );
    assert!(changed);
    assert!(t.delivered_in.is_none());
    assert_eq!(t.delivered_repo.as_deref(), Some("git@h:c.git"));
}

#[test]
fn describe_falls_back_to_short_sha_when_no_subject() {
    // A tempdir isn't a git repo, so both subject and short-sha
    // lookups return None — describe falls back to the raw sha.
    let dir = TempDir::new().unwrap();
    let out = describe(dir.path(), "deadbeef");
    assert_eq!(out, "deadbeef");
}

#[test]
fn local_hit_via_hint_sets_resolved_repo_to_current() {
    // A local resolve via the hint path must populate
    // `resolved_repo` from the local clone, so a JSON consumer can
    // always tell which repo the sha came from. (bl-f37b)
    let (dir, sha) = local_repo_with_tag("bl-abcd");
    let mut t = empty_task();
    t.delivered_in = Some(sha.clone());
    let d = resolve(dir.path(), "main", &t);
    assert_eq!(d.sha.as_deref(), Some(sha.as_str()));
    assert!(!d.hint_stale);
    assert_eq!(d.resolved_repo, Some(crate::repo_url::current(dir.path())));
}

#[test]
fn local_hit_via_tag_scan_sets_resolved_repo() {
    // No hint set: the tag-scan branch resolves the sha and must
    // also stamp the resolved_repo with the current clone.
    let (dir, sha) = local_repo_with_tag("bl-abcd");
    let d = resolve(dir.path(), "main", &empty_task());
    assert_eq!(d.sha.as_deref(), Some(sha.as_str()));
    assert_eq!(d.resolved_repo, Some(crate::repo_url::current(dir.path())));
}

#[test]
fn resolve_with_remote_off_ignores_delivered_repo() {
    // Even with a fetchable `delivered_repo`, the default opts.remote
    // = false path must not engage the remote fallback — single-repo
    // callers stay byte-identical with the legacy `resolve`.
    let (src, _sha) = local_repo_with_tag("bl-abcd");
    let url = src.path().to_string_lossy().into_owned();
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some(url);

    let d = resolve(local_root.path(), "main", &t);
    assert!(d.sha.is_none());
    assert!(d.resolved_repo.is_none());
}

#[test]
fn resolve_with_remote_falls_back_when_delivered_repo_set() {
    // Local clone has no history; the remote opt-in plus a fetchable
    // `delivered_repo` should produce the sha and tag the resolved
    // repo with the URL we resolved through.
    let (src, sha) = local_repo_with_tag("bl-abcd");
    let url = src.path().to_string_lossy().into_owned();
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some(url.clone());

    let d = resolve_with(
        local_root.path(),
        "main",
        &t,
        ResolveOpts { remote: true },
    );
    assert_eq!(d.sha.as_deref(), Some(sha.as_str()));
    assert_eq!(d.resolved_repo, Some(url));
}

#[test]
fn resolve_with_remote_on_no_delivered_repo_is_local_only() {
    // remote opt-in is meaningless when the task has no
    // delivered_repo to chase. Behavior must equal the local-only
    // result.
    let local_root = TempDir::new().unwrap();
    let d = resolve_with(
        local_root.path(),
        "main",
        &empty_task(),
        ResolveOpts { remote: true },
    );
    assert!(d.sha.is_none());
    assert!(d.resolved_repo.is_none());
}

#[test]
fn resolve_with_remote_unreachable_url_returns_local_result() {
    // Soft-fail: a delivered_repo we can't fetch must not break the
    // command — sha stays None, resolved_repo stays None, hint_stale
    // follows the local scan (no hint → false).
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some("/no/such/path/repo.git".into());

    let d = resolve_with(
        local_root.path(),
        "main",
        &t,
        ResolveOpts { remote: true },
    );
    assert!(d.sha.is_none());
    assert!(!d.hint_stale);
    assert!(d.resolved_repo.is_none());
}
