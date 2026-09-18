//! Tests for §12's PUBLISH half — `push`: the ordinary publish, the reconcile
//! a non-ff reject runs (bl-21ab: a different-ball race lands, a same-ball
//! race is E5 naming the ball, an unreachable remote fails open, a denied push
//! stays fail-closed), the §16 not-yet-cut-over skip (bl-868d), and the
//! held-anvil rule (bl-1266/bl-aac7). Split from `remote_ops_tests.rs` (which
//! keeps `sync` and the install config fetch) to stay under the 300-line cap.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, empty_remote, env_held, env_top, legacy_remote, local_unpushed,
    remote_with_branch, revoked_remote, store_clone, tip, BRANCH,
};
use tempfile::TempDir;

#[test]
fn push_keeps_work_local_when_the_remote_tip_is_not_a_store() {
    // bl-868d: publishing over an un-cut-over legacy ref is rejected (non-ff,
    // unrelated histories) — that is the migration window, not split-brain:
    // warn, keep the work local, and NEVER rewrite the legacy ref (cutover is
    // the runbook's explicit history join + fast-forward push). A rejected
    // push to a GREENFIELD store stays the E5 error (the test below).
    let tmp = TempDir::new().unwrap();
    let remote = legacy_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    let before = tip(&remote, BRANCH);
    push(&binding(Some(&remote), &store), &env_top()).unwrap();
    assert_eq!(tip(&remote, BRANCH), before); // the legacy ref was not rewritten
}

#[test]
fn push_to_an_unreachable_remote_fails_open_and_keeps_the_store_ahead() {
    // bl-21ab: transport failure is a non-event — warn, keep the seal local, the
    // store reads ahead until a reachable op or `bl sync` publishes. Aborting
    // here would un-seal perfectly good work over a hub that is merely down.
    let tmp = TempDir::new().unwrap();
    let store = local_unpushed(tmp.path());
    let sealed = tip(&store, "HEAD");
    let gone = tmp.path().join("no-such-remote.git");
    push(&binding(Some(&gone), &store), &env_top()).unwrap();
    assert_eq!(tip(&store, "HEAD"), sealed, "the seal stays, unpublished");
}

#[test]
fn push_publishes_the_local_balls_branch_to_the_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let landed = commit(&store, "landed.txt", "landed");

    push(&binding(Some(&remote), &store), &env_top()).unwrap();
    assert_eq!(tip(&remote, BRANCH), landed);
}

/// bl-1266: an op whose store an ENCLOSING `bl` holds open does not publish —
/// the enclosing op's own trailing push carries the seal, so a parent that
/// aborts afterwards is un-sealed by a purely local `git reset` with nothing
/// left on the remote to chase. Identical to the test above, but the held chain
/// says a `bl` above the spawner holds this same store.
#[test]
fn push_from_a_nested_op_on_a_held_store_publishes_nothing() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let before = tip(&remote, BRANCH);
    let landed = commit(&store, "landed.txt", "landed");
    assert_ne!(landed, before, "the fixture must leave something worth publishing");

    push(&binding(Some(&remote), &store), &env_held(&[&store, &store])).unwrap();

    assert_eq!(tip(&remote, BRANCH), before, "a nested op must not publish a held anvil");
}

/// bl-aac7 (bl-1266's H1 fill): nesting is STORE-scoped. A `bl -C` shelled by a
/// plugin addresses a DIFFERENT store — no enclosing op holds it, so no parent
/// push will ever carry its seal; suppressing it (as the old depth predicate
/// did) left the far store sealed-but-unpublished forever. It publishes.
#[test]
fn push_from_a_nested_op_on_an_unheld_store_publishes() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let landed = commit(&store, "landed.txt", "landed");
    let other = tmp.path().join("elsewhere/tasks");

    push(&binding(Some(&remote), &store), &env_held(&[&other, &store])).unwrap();

    assert_eq!(tip(&remote, BRANCH), landed, "an unheld anvil is this op's to publish");
}

#[test]
fn push_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let remote = empty_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    push(&binding(None, &store), &env_top()).unwrap();
    // The empty remote still has no balls branch.
    assert!(git(&remote, &["rev-parse", BRANCH]).is_err());
}

#[test]
fn a_rejected_push_reconciles_a_different_ball_race_and_lands() {
    // bl-3616 §3 / bl-21ab: the non-ff reject is the contention check firing,
    // and the answer is ONE reconcile — fetch, rebase the in-flight seal onto
    // the remote tip, push again. Seals on different balls rebase clean by
    // construction, so the op lands with no human in the loop: the remote ends
    // at the rebased local tip carrying BOTH sides' files.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "tasks/bl-aaaa.md", "theirs");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    let sealed = commit(&store, "tasks/bl-bbbb.md", "ours");

    push(&binding(Some(&remote), &store), &env_top()).unwrap();

    let landed = tip(&store, "HEAD");
    assert_ne!(landed, sealed, "the seal was rebased — its sha is scratch (bl-3616 §6.1)");
    assert_eq!(tip(&remote, BRANCH), landed, "the rebased seal published");
    assert!(git(&store, &["cat-file", "-e", "HEAD:tasks/bl-aaaa.md"]).is_ok(), "theirs arrived");
    assert!(git(&store, &["cat-file", "-e", "HEAD:tasks/bl-bbbb.md"]).is_ok(), "ours survived");
    assert!(git(&store, &["rev-parse", "--verify", "-q", "REBASE_HEAD"]).is_err(), "no rebase in progress");
}

#[test]
fn a_rejected_push_refuses_a_same_ball_race_naming_the_ball() {
    // The one contention that is semantically meaningful: both sides changed
    // tasks/<id>.md (two claims of one ball, a close over a remote update).
    // The rebase is aborted — the local seal is exactly as it was, nothing was
    // published — and E5 names the ball and both exits (bl-21ab).
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "tasks/bl-c0de.md", "claimant = other");
    let published = git(&other, &["push", "-q", "origin", BRANCH]).map(|_| tip(&remote, BRANCH)).unwrap();
    let sealed = commit(&store, "tasks/bl-c0de.md", "claimant = me");

    let err = push(&binding(Some(&remote), &store), &env_top()).unwrap_err().to_string();
    assert!(err.contains("push rejected"), "{err}");
    assert!(err.contains("bl-c0de changed on both sides"), "{err}");
    assert!(err.contains("nothing was published and nothing local was changed"), "{err}");
    assert!(err.contains("run `bl sync`, then re-run it"), "{err}");
    assert!(err.contains(&format!("the store checkout, {}:", store.display())), "{err}");
    assert!(err.contains("`git rebase FETCH_HEAD`"), "{err}");
    for raw in ["CONFLICT", "hint:", "could not apply", "Could not apply"] {
        assert!(!err.contains(raw), "raw git leaked ({raw}): {err}");
    }
    assert_eq!(tip(&store, "HEAD"), sealed, "the seal stays for core to un-seal");
    assert_eq!(tip(&remote, BRANCH), published, "the remote is untouched");
    assert!(git(&store, &["rev-parse", "--verify", "-q", "REBASE_HEAD"]).is_err(), "the rebase was aborted");
    assert!(git(&store, &["ls-files", "-u"]).unwrap().is_empty(), "no unmerged paths left behind");
}

#[test]
fn a_close_over_a_remote_update_is_the_same_named_refusal() {
    // Modify/delete: the remote updated the ball, this op closed (deleted) it.
    // Not auto-resolvable — whether the update mattered is a human's call — so
    // it refuses exactly like modify/modify, naming the ball from the unmerged
    // index (the delete side has no content to name it by).
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    commit(&checkout(tmp.path(), &remote, "seed2"), "tasks/bl-dead.md", "born");
    git(&tmp.path().join("seed2"), &["push", "-q", "origin", BRANCH]).unwrap();
    let store = store_clone(tmp.path(), &remote);
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "tasks/bl-dead.md", "updated remotely");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    git(&store, &["rm", "-q", "tasks/bl-dead.md"]).unwrap();
    git(&store, &["commit", "-q", "-m", "close bl-dead"]).unwrap();

    let err = push(&binding(Some(&remote), &store), &env_top()).unwrap_err().to_string();
    assert!(err.contains("bl-dead changed on both sides"), "{err}");
}

#[cfg(unix)]
#[test]
fn a_push_denied_after_a_clean_reconcile_stays_fail_closed() {
    // Revoked permissions / a server hook: the fetch works and the rebase is a
    // no-op, so a second reject is NOT contention — it is the one reject that
    // must still abort the op (silently degrading here would be split-brain).
    let tmp = TempDir::new().unwrap();
    let remote = revoked_remote(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    commit(&store, "tasks/bl-0001.md", "sealed");
    let err = push(&binding(Some(&remote), &store), &env_top()).unwrap_err().to_string();
    assert!(err.contains("even after reconciling"), "{err}");
    assert!(err.contains("permissions, a server hook"), "{err}");
}
