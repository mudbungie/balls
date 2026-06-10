//! Tests for the §12/§13 remote ops — sync (ff import + the contention
//! signal), push (publish + the E5 reject), the §16 not-yet-cut-over skips
//! (bl-868d), and the install config fetch.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, empty_remote, legacy_remote, local_unpushed, store_clone,
    remote_with_branch, remote_with_config, tip, BRANCH,
};
use tempfile::TempDir;

#[test]
fn sync_fast_forwards_store_onto_the_advanced_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    // A second checkout advances the remote out from under the store.
    let other = checkout(tmp.path(), &remote, "other");
    let moved = commit(&other, "next.txt", "next");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

    sync(&binding(Some(&remote), &store)).unwrap();
    assert_eq!(tip(&store, "HEAD"), moved);
}

#[test]
fn sync_of_an_upstream_less_branch_is_a_no_op_the_landing_for_free() {
    // §13: "fetch a branch's upstream, if any" — the remote carries no
    // `balls/config`, so syncing the landing BY ITS REAL NAME fetches
    // nothing and ff's nothing. No token is special-cased; any local-only
    // branch takes the same no-op path.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let before = tip(&store, "HEAD");
    for upstream_less in [crate::LANDING_BRANCH, "work/bl-0000"] {
        let mut b = binding(Some(&remote), &store);
        b.tasks_branch = upstream_less.into();
        sync(&b).unwrap();
        assert_eq!(tip(&store, "HEAD"), before);
    }
}

#[test]
fn sync_of_a_non_checked_out_branch_ffs_that_branch_not_the_checkout() {
    // §13: the ff target is the branch the binding NAMES. The store sits on
    // `balls`; syncing `other` moves refs/heads/other (a pure ref move) and
    // leaves the checked-out branch where it was.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let seat = checkout(tmp.path(), &remote, "seat");
    git(&seat, &["checkout", "-q", "-b", "other"]).unwrap();
    let moved = commit(&seat, "other.txt", "other");
    git(&seat, &["push", "-q", "origin", "other"]).unwrap();

    let head = tip(&store, "HEAD");
    let mut b = binding(Some(&remote), &store);
    b.tasks_branch = "other".into();
    sync(&b).unwrap();
    assert_eq!(tip(&store, "other"), moved); // the named branch moved…
    assert_eq!(tip(&store, "HEAD"), head); // …the checkout did not
}

#[test]
fn sync_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let before = tip(&store, "HEAD");
    sync(&binding(None, &store)).unwrap();
    assert_eq!(tip(&store, "HEAD"), before);
}

#[test]
fn sync_fails_on_a_non_fast_forward_the_contention_signal() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    // Diverge: a local commit AND a remote commit off the same base.
    commit(&store, "local.txt", "local");
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "remote.txt", "remote");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

    let err = sync(&binding(Some(&remote), &store)).unwrap_err();
    assert!(err.to_string().contains("git merge --ff-only"));
}

#[test]
fn sync_skips_a_not_yet_cut_over_legacy_upstream_instead_of_failing() {
    // bl-868d: the hub's `balls/tasks` is still the PRE-greenfield legacy
    // store (no `tasks/` at its tip) — not a store upstream at all, so the
    // failed ff is the §16 migration window, not contention: warn and no-op,
    // leaving the local greenfield store exactly where it was.
    let tmp = TempDir::new().unwrap();
    let remote = legacy_remote(tmp.path());
    let store = local_unpushed(tmp.path()); // the founded greenfield orphan
    let before = tip(&store, "HEAD");
    sync(&binding(Some(&remote), &store)).unwrap();
    assert_eq!(tip(&store, "HEAD"), before);
}

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
    push(&binding(Some(&remote), &store)).unwrap();
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
    let err = push(&b).unwrap_err().to_string();
    assert!(err.contains("git push"), "{err}");
    assert!(!err.contains("re-run after `bl sync`"), "{err}");
}

#[test]
fn push_publishes_the_local_balls_branch_to_the_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let landed = commit(&store, "landed.txt", "landed");

    push(&binding(Some(&remote), &store)).unwrap();
    assert_eq!(tip(&remote, BRANCH), landed);
}

#[test]
fn fetch_config_brings_the_centers_config_to_the_landing_fetch_head() {
    let tmp = TempDir::new().unwrap();
    let center = remote_with_config(tmp.path(), "balls/shared");
    let landing = local_unpushed(tmp.path()); // any local git repo to fetch into
    let mut b = binding(Some(&center), &landing);
    b.landing = landing.to_string_lossy().into_owned();
    fetch_config(&b).unwrap();
    // FETCH_HEAD in the landing now carries the center's config branch.
    let cfg = git(&landing, &["show", "FETCH_HEAD:config/balls.toml"]).unwrap();
    assert!(cfg.contains("balls/shared"), "fetched config: {cfg}");
}

#[test]
fn fetch_config_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let landing = local_unpushed(tmp.path());
    let mut b = binding(None, &landing);
    b.landing = landing.to_string_lossy().into_owned();
    fetch_config(&b).unwrap(); // no remote → nothing fetched, no error
    assert!(git(&landing, &["rev-parse", "FETCH_HEAD"]).is_err());
}

#[test]
fn fetch_config_when_the_remote_lacks_the_landing_is_a_no_op() {
    // bl-45fd: the landing is never pushed by bl (§4 single-owner), so a
    // stock hub carries no `balls/config`. A present remote MISSING the
    // ref is §13's "upstream, if any" no-op — not a fatal abort of a
    // purely local install. Only an adopt naming the center as --from
    // needs the fetch, and that fails at point-of-use (no FETCH_HEAD).
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path()); // carries `balls`, no `balls/config`
    let landing = local_unpushed(tmp.path());
    let mut b = binding(Some(&remote), &landing);
    b.landing = landing.to_string_lossy().into_owned();
    fetch_config(&b).unwrap(); // ref absent → nothing fetched, no error
    assert!(git(&landing, &["rev-parse", "FETCH_HEAD"]).is_err());
}

#[test]
fn push_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let remote = empty_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    push(&binding(None, &store)).unwrap();
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
    // never a raw non-ff dump alone.
    let err = push(&binding(Some(&remote), &store)).unwrap_err().to_string();
    assert!(err.contains("push rejected by the established remote store"), "{err}");
    assert!(err.contains("re-run after `bl sync`"), "{err}");
}

#[test]
fn sync_refuses_an_option_like_branch_before_touching_git() {
    // A config-sourced branch that begins with `-` (e.g. `--upload-pack=…`)
    // is refused as option-injection, not handed to `git fetch` (bl-2d6d).
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let mut b = binding(Some(&remote), &store);
    b.tasks_branch = "--upload-pack=evil".into();
    let err = sync(&b).unwrap_err().to_string();
    assert!(err.contains("looks like an option"), "{err}");
}
