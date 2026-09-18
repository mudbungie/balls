//! Tests for §12/§13's `sync` — the reconcile over every unpublished seal
//! (bl-21ab: a plain ff, a diverged store rebased and published, a same-ball
//! conflict refused by name with the seals left local, an unreachable remote
//! failing open, the §16 not-yet-cut-over skip) and the install config fetch.
//! `push` and its rules live in the sibling `remote_ops_push_tests.rs`.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, env_held, env_top, legacy_remote, local_unpushed,
    remote_with_branch, remote_with_config, store_clone, tip, BRANCH,
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

    sync(&binding(Some(&remote), &store), &env_top()).unwrap();
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
        sync(&b, &env_top()).unwrap();
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
    sync(&b, &env_top()).unwrap();
    assert_eq!(tip(&store, "other"), moved); // the named branch moved…
    assert_eq!(tip(&store, "HEAD"), head); // …the checkout did not
}

#[test]
fn sync_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let before = tip(&store, "HEAD");
    sync(&binding(None, &store), &env_top()).unwrap();
    assert_eq!(tip(&store, "HEAD"), before);
}

#[test]
fn sync_rebases_a_diverged_stores_seals_onto_the_remote_and_publishes() {
    // bl-3616 §3: `bl sync` IS publication when the tracker is not wired on
    // `*.post` — the reconcile over N unpublished seals. Diverged on different
    // balls, the local seals rebase clean and land; the store ends at one line
    // of history the remote also has, with both sides' files.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let held1 = commit(&store, "tasks/bl-1111.md", "local one");
    commit(&store, "tasks/bl-2222.md", "local two");
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "tasks/bl-3333.md", "remote");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();

    sync(&binding(Some(&remote), &store), &env_top()).unwrap();

    let head = tip(&store, "HEAD");
    assert_eq!(tip(&remote, BRANCH), head, "the rebased seals published");
    assert!(git(&store, &["merge-base", "--is-ancestor", &held1, "HEAD"]).is_err(), "the old seal sha is gone");
    for f in ["bl-1111", "bl-2222", "bl-3333"] {
        assert!(git(&store, &["cat-file", "-e", &format!("HEAD:tasks/{f}.md")]).is_ok(), "{f}");
    }
    // The subjects survived in order, oldest first — content and order are what a
    // rebase keeps; only the shas are scratch (bl-3616 §6.1).
    let log = git(&store, &["log", "--format=%s", "-3"]).unwrap();
    assert_eq!(log.lines().collect::<Vec<_>>(), ["local two", "local one", "remote"]);
}

#[test]
fn sync_refuses_a_same_ball_conflict_by_name_and_leaves_every_seal_local() {
    // The one rebase that must not be auto-resolved: the same tasks/<id>.md
    // changed on both sides. Aborted — every local seal exactly as it was, the
    // remote untouched, no rebase in progress — and the refusal names the ball,
    // the unpublished set, and both exits. Never git's CONFLICT dump.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    commit(&store, "tasks/bl-4444.md", "unrelated local seal");
    let held = commit(&store, "tasks/bl-5555.md", "mine");
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "tasks/bl-5555.md", "theirs");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    let published = tip(&remote, BRANCH);

    let err = sync(&binding(Some(&remote), &store), &env_top()).unwrap_err().to_string();
    assert!(err.contains(&format!("`{BRANCH}` moved and bl-5555 changed on both sides")), "{err}");
    assert!(err.contains("nothing was published and nothing local was changed"), "{err}");
    assert!(err.contains(&format!("the store checkout, {}: `git log FETCH_HEAD..{BRANCH}` lists them", store.display())), "{err}");
    assert!(err.contains("rebase FETCH_HEAD`, resolve the named file, then `bl sync` publishes"), "{err}");
    assert!(err.contains("reset --hard FETCH_HEAD"), "{err}");
    assert!(!err.contains("CONFLICT") && !err.contains("hint:"), "raw git leaked: {err}");
    assert_eq!(tip(&store, "HEAD"), held, "every local seal stays local");
    assert_eq!(tip(&remote, BRANCH), published);
    assert!(git(&store, &["rev-parse", "--verify", "-q", "REBASE_HEAD"]).is_err(), "aborted, not wedged");
    // The advertised exit has an exit: take the remote's version, and the next
    // sync is a clean no-op that publishes nothing new.
    git(&store, &["reset", "-q", "--hard", "FETCH_HEAD"]).unwrap();
    sync(&binding(Some(&remote), &store), &env_top()).unwrap();
    assert_eq!(tip(&store, "HEAD"), published);
}

#[test]
fn sync_to_an_unreachable_remote_fails_open() {
    // bl-21ab: an unreachable hub is a non-event for sync too — warn, keep the
    // local seals, exit 0. The store reads ahead until the hub is back.
    let tmp = TempDir::new().unwrap();
    let store = local_unpushed(tmp.path());
    let before = tip(&store, "HEAD");
    let gone = tmp.path().join("no-such-remote.git");
    sync(&binding(Some(&gone), &store), &env_top()).unwrap();
    assert_eq!(tip(&store, "HEAD"), before);
}

#[test]
fn sync_from_a_nested_op_rebases_but_leaves_the_push_to_the_enclosing_op() {
    // bl-1266 holds inside the reconcile: a nested `bl sync` on a held store
    // brings the checkout current (the rebase is local) but the enclosing op
    // owns the publish — the remote tip does not move.
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    commit(&store, "tasks/bl-6666.md", "local");
    let published = tip(&remote, BRANCH);
    sync(&binding(Some(&remote), &store), &env_held(&[&store, &store])).unwrap();
    assert_eq!(tip(&remote, BRANCH), published, "not this op's to publish");
    assert!(git(&store, &["cat-file", "-e", "HEAD:tasks/bl-6666.md"]).is_ok());
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
    sync(&binding(Some(&remote), &store), &env_top()).unwrap();
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
    let err = sync(&b, &env_top()).unwrap_err().to_string();
    assert!(err.contains("looks like an option"), "{err}");
}
