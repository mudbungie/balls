//! Tests for §12's PUBLISH half — `push`: the ordinary publish, the E5 non-ff
//! reject and the recovery it advertises, the §16 not-yet-cut-over skip
//! (bl-868d), and the outermost-`bl`-only rule (bl-1266). Split from
//! `remote_ops_tests.rs` (which keeps `sync` and the install config fetch) to
//! stay under the 300-line cap.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, empty_remote, env_at, legacy_remote, local_unpushed,
    remote_with_branch, store_clone, tip, BRANCH,
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
    push(&binding(Some(&remote), &store), &env_at(1)).unwrap();
    assert_eq!(tip(&remote, BRANCH), before); // the legacy ref was not rewritten
}

#[test]
fn push_to_an_unreachable_remote_stays_the_original_error() {
    // The not-yet-cut-over skip needs POSITIVE identification of a non-store
    // tip; when even the shape read fails (no such remote), the push's own
    // error surfaces — never a silent skip, and never misnamed as the E5
    // established-store reject (bl-3ddb).
    let tmp = TempDir::new().unwrap();
    let store = local_unpushed(tmp.path());
    let gone = tmp.path().join("no-such-remote.git");
    let mut b = binding(Some(&gone), &store);
    b.remote = Some(gone.to_string_lossy().into_owned());
    let err = push(&b, &env_at(1)).unwrap_err().to_string();
    assert!(err.contains("git push"), "{err}");
    assert!(!err.contains("bl sync"), "{err}");
}

#[test]
fn push_publishes_the_local_balls_branch_to_the_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let landed = commit(&store, "landed.txt", "landed");

    push(&binding(Some(&remote), &store), &env_at(1)).unwrap();
    assert_eq!(tip(&remote, BRANCH), landed);
}

/// bl-1266: an op that is NOT the outermost `bl` in its invocation tree does not
/// publish — the enclosing op's own trailing push carries the seal, so a parent
/// that aborts afterwards is un-sealed by a purely local `git reset` with nothing
/// left on the remote to chase. Identical to the test above but at depth 2.
#[test]
fn push_from_a_nested_op_publishes_nothing() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let before = tip(&remote, BRANCH);
    let landed = commit(&store, "landed.txt", "landed");
    assert_ne!(landed, before, "the fixture must leave something worth publishing");

    push(&binding(Some(&remote), &store), &env_at(2)).unwrap();

    assert_eq!(tip(&remote, BRANCH), before, "a nested op must not publish");
}

#[test]
fn push_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let remote = empty_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    push(&binding(None, &store), &env_at(1)).unwrap();
    // The empty remote still has no balls branch.
    assert!(git(&remote, &["rev-parse", BRANCH]).is_err());
}

#[test]
fn push_fails_when_the_remote_rejects_a_non_fast_forward() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    // Remote moves ahead; the store's divergent commit can't ff-push.
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "remote.txt", "remote");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    commit(&store, "local.txt", "local");

    // E5 (bl-3ddb): a reject by an ESTABLISHED store names the catalog remedy,
    // never a raw non-ff dump alone. The remedy is the two-step recovery
    // (bl-547f) — `bl sync`, then re-run — so the half-close reads recoverable,
    // but it FORWARDS to sync's verdict rather than promising it (bl-4945).
    let err = push(&binding(Some(&remote), &store), &env_at(1)).unwrap_err().to_string();
    assert!(err.contains("push rejected: the remote store moved ahead"), "{err}");
    assert!(err.contains("run `bl sync`"), "{err}");
    assert!(err.contains("or refuses and names what this store holds"), "{err}");
    assert!(err.contains("then re-run the command"), "{err}");
}

#[test]
fn the_recovery_e5_advertises_exits_a_sealed_but_unpublished_store() {
    // bl-4945: E5's two-step converges only because a rejected push UN-SEALS,
    // leaving the store behind the remote and sync's ff-only free to run. A
    // store that ALREADY carries an unpublished commit — a crash between seal
    // and push, the bl-547f half-close shape — stays diverged past the un-seal,
    // so sync refuses (correctly) and an unconditional "sync, then re-run"
    // would loop forever. E5 forwards to sync's verdict, sync's refusal names
    // the exit, and this drives that advertised recipe end to end.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let stranded = commit(&store, "sealed.txt", "sealed, never published");
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "remote.txt", "remote");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

    // E5 promises no convergence — it hands the operator to sync…
    let e5 = push(&binding(Some(&remote), &store), &env_at(1)).unwrap_err().to_string();
    assert!(e5.contains("or refuses and names what this store holds"), "{e5}");
    // …which refuses, and names the unpublished set plus both ways out.
    let refusal = sync(&binding(Some(&remote), &store)).unwrap_err().to_string();
    let listing = format!("git -C {} log FETCH_HEAD..{BRANCH}", store.display());
    assert!(refusal.contains(&listing), "{refusal}");
    assert!(refusal.contains("rebase them onto FETCH_HEAD and push"), "{refusal}");
    assert!(refusal.contains("reset --hard"), "{refusal}");

    // Follow it: the listed set is exactly the stranded commit, and republishing
    // it converges — the advertised recovery has an exit, and reaching it needs
    // no balls verb balls does not have.
    let unpublished = git(&store, &["log", "--format=%H", &format!("FETCH_HEAD..{BRANCH}")]).unwrap();
    assert_eq!(unpublished, stranded);
    git(&store, &["rebase", "FETCH_HEAD"]).unwrap();
    push(&binding(Some(&remote), &store), &env_at(1)).unwrap();
    sync(&binding(Some(&remote), &store)).unwrap(); // converged: a clean no-op
}

