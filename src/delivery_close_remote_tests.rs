//! Unit tests for `populate_on_close`'s bl-e454 `resolve_remote` knob.
//! Split out of `delivery_tests.rs` to keep that file under the
//! 300-line cap; shares fixtures via `delivery_test_support`. The
//! local-only `populate_on_close` paths (manual override, hint
//! present, scan miss, manual_repo) stay next to the rest of the
//! delivery suite — these cover only the remote fallback.

use super::*;
use crate::delivery_test_support::{empty_task, local_repo_with_tag};
use tempfile::TempDir;

#[test]
fn populate_on_close_remote_off_ignores_delivered_repo() {
    // Default `resolve_remote = false` matches the byte-identical
    // single-repo close: even when the task has a fetchable
    // delivered_repo and the local clone has no `[id]` history, the
    // remote fallback must stay dormant — no fetch, sha stays null.
    let (src, _sha) = local_repo_with_tag("bl-abcd");
    let url = src.path().to_string_lossy().into_owned();
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some(url.clone());

    let changed = populate_on_close(local_root.path(), "main", &mut t, None, None, false);
    assert!(!changed);
    assert!(t.delivered_in.is_none());
    // delivered_repo on the task is left untouched — caller set it.
    assert_eq!(t.delivered_repo.as_deref(), Some(url.as_str()));
}

#[test]
fn populate_on_close_remote_on_resolves_via_delivered_repo() {
    // bl-e454: the write-side parallel to bl-f37b. Local clone has no
    // history and no `[id]` commit; opting in to remote resolution with
    // a fetchable delivered_repo writes the sha into the task. The
    // auto-tag of `delivered_repo` from `repo_url::current` is the
    // local clone's value, NOT the source URL — `populate_on_close`'s
    // auto-tag policy is "this clone produced it" by default. The
    // task already carried the source URL before close.
    let (src, sha) = local_repo_with_tag("bl-abcd");
    let url = src.path().to_string_lossy().into_owned();
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some(url.clone());

    let changed = populate_on_close(local_root.path(), "main", &mut t, None, None, true);
    assert!(changed);
    assert_eq!(t.delivered_in.as_deref(), Some(sha.as_str()));
    // populate_on_close overwrites delivered_repo with the current
    // clone whenever it sets delivered_in — but here the current
    // clone's `repo_url::current(local_root)` is the bridge/hub, not
    // the source URL. The source URL is still recoverable from the
    // pre-close state on the state branch; the auto-tag policy is
    // documented in bl-7523 and stays consistent.
    assert_eq!(
        t.delivered_repo.as_deref(),
        Some(crate::repo_url::current(local_root.path()).as_str())
    );
}

#[test]
fn populate_on_close_remote_on_unreachable_url_stays_null() {
    // Soft-fail mirrors the show path: a delivered_repo we can't fetch
    // must not break `bl close`. sha stays null, the close commit lands,
    // and the task archives without provenance — same as today.
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some("/no/such/path/repo.git".into());

    let changed = populate_on_close(local_root.path(), "main", &mut t, None, None, true);
    assert!(!changed);
    assert!(t.delivered_in.is_none());
}

#[test]
fn populate_on_close_remote_on_manual_sha_skips_fetch() {
    // `--delivered <sha>` still wins unconditionally — when the
    // operator hands the sha, no scan and no fetch run, even with
    // `--resolve-remote` set.
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();
    t.delivered_repo = Some("/no/such/path/repo.git".into());

    let changed = populate_on_close(
        local_root.path(),
        "main",
        &mut t,
        Some("forced".into()),
        None,
        true,
    );
    assert!(changed);
    assert_eq!(t.delivered_in.as_deref(), Some("forced"));
}

#[test]
fn populate_on_close_remote_on_without_delivered_repo_is_local_only() {
    // The remote opt-in is a no-op when the task carries no
    // delivered_repo — there's nothing to fetch. Behavior must equal
    // the local-only miss path: no sha, no provenance, no change.
    let local_root = TempDir::new().unwrap();
    let mut t = empty_task();

    let changed = populate_on_close(local_root.path(), "main", &mut t, None, None, true);
    assert!(!changed);
    assert!(t.delivered_in.is_none());
    assert!(t.delivered_repo.is_none());
}
