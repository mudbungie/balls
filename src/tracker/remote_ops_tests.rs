//! Tests for §12/§13's IMPORT half — `sync` (ff import, the contention signal
//! in balls' voice, the §16 not-yet-cut-over skip) and the install config fetch.
//! `push` and its rules live in the sibling `remote_ops_push_tests.rs`.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, legacy_remote, local_unpushed, remote_with_branch,
    remote_with_config, store_clone, tip, BRANCH,
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
fn sync_refusing_a_non_fast_forward_speaks_balls_voice_not_gits() {
    // bl-3129: the ff-only IS §13's detect-and-act and refusing is it working —
    // but raw git ("fatal: Not possible to fast-forward, aborting") reads as
    // damage rather than as the two facts it is. The refusal names the moved
    // remote, that nothing was imported or changed, and BOTH readings of a
    // re-run: convergence once an in-flight seal settles, or a store that
    // really does hold commits the remote never took.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    // Diverge: a local commit AND a remote commit off the same base.
    let held = commit(&store, "local.txt", "local");
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "remote.txt", "remote");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

    let err = sync(&binding(Some(&remote), &store)).unwrap_err().to_string();
    assert!(err.contains(&format!("`{BRANCH}` moved")), "{err}");
    assert!(err.contains("could not take the fast-forward"), "{err}");
    assert!(err.contains("nothing was imported and nothing local was changed"), "{err}");
    assert!(err.contains("Re-run `bl sync`"), "{err}");
    assert!(err.contains("carries commits the remote never took"), "{err}");
    // …and, since bl-4945, the EXIT for that second reading: naming the state
    // without naming a way out still loops.
    assert!(err.contains("Reconciling those is yours"), "{err}");
    assert!(err.contains("balls never merges the two histories"), "{err}");
    assert!(!err.contains("Not possible to fast-forward"), "raw git leaked: {err}");
    assert!(!err.contains("git merge"), "raw git leaked: {err}");
    // And the claim holds: the refusal imported nothing, so the store still
    // sits on its own tip with the local commit intact.
    assert_eq!(tip(&store, "HEAD"), held);
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
