//! Tests for the §12 prime handlers — the stealth self-lock, `prime/pre`
//! clone-in (established/present/bootstrap), `prime/post` founding vs publish
//! (and their rejection paths), and the store-elsewhere diagnostic.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, default_binding, empty_remote, env, landing_repo, local_unpushed,
    remote_with_branch, remote_with_config, store_clone, tip, tracked, unpushable_remote, BRANCH,
};
use tempfile::TempDir;

#[test]
fn stealth_writes_a_self_lock_and_touches_no_remote() {
    let tmp = TempDir::new().unwrap();
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    let b = binding(None, &tmp.path().join("landing"));
    prime(&b, &env).unwrap();
    let lock = env.xdg.clone_dir(Path::new(&b.invocation_path)).root().join("stealth.lock");
    assert!(lock.is_file());
}

#[test]
fn prime_pre_clones_an_established_remote_store_into_a_local_ref() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let landing = landing_repo(tmp.path()); // on balls/config, no `balls` store branch yet
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    // The remote already carries the store branch and this clone lacks it →
    // clone-in fetches it into a local ref so core's materialize checks it out
    // (no orphan, no reset — bl-0a23).
    prime(&tracked(&remote, &landing, &landing), &env).unwrap();
    assert_eq!(tip(&landing, BRANCH), tip(&remote, BRANCH));
}

#[test]
fn prime_pre_skips_clone_in_when_the_local_branch_is_already_present() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let landing = store_clone(tmp.path(), &remote); // a prior clone — has `balls`
    let before = tip(&landing, BRANCH);
    // Advance the remote; a clone-in WOULD pull it. A present local branch is
    // left for `sync` to ff, so prime/pre must NOT fetch it.
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "next.txt", "next");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    prime(&tracked(&remote, &landing, &landing), &env).unwrap();
    assert_eq!(tip(&landing, BRANCH), before); // unchanged — clone-in skipped
}

#[test]
fn prime_pre_in_a_bootstrap_clones_nothing() {
    let tmp = TempDir::new().unwrap();
    let remote = empty_remote(tmp.path()); // no store branch — bootstrap
    let landing = landing_repo(tmp.path());
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    prime(&tracked(&remote, &landing, &landing), &env).unwrap();
    // The store branch stays absent — core founds it, prime/post pushes it.
    assert!(!local_branch(&landing, BRANCH));
}

#[test]
fn prime_post_founds_an_absent_remote_by_pushing() {
    let tmp = TempDir::new().unwrap();
    let remote = empty_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    prime_post(&binding(Some(&remote), &store), &env).unwrap();
    assert_eq!(tip(&remote, BRANCH), tip(&store, "HEAD")); // the push created the branch
}

#[test]
fn prime_post_brings_current_then_publishes_to_an_established_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let landed = commit(&store, "landed.txt", "landed"); // local work ahead of the remote
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    prime_post(&binding(Some(&remote), &store), &env).unwrap();
    assert_eq!(tip(&remote, BRANCH), landed); // fetch-ff is a no-op, the push publishes
}

#[test]
fn prime_post_founding_reject_falls_back_to_stealth_local() {
    let tmp = TempDir::new().unwrap();
    let remote = unpushable_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    let b = binding(Some(&remote), &store);
    prime_post(&b, &env).unwrap(); // push denied on an absent branch → silent stealth
    let lock = env.xdg.clone_dir(Path::new(&b.invocation_path)).root().join("stealth.lock");
    assert!(lock.is_file());
    assert!(git(&remote, &["rev-parse", BRANCH]).is_err()); // nothing was founded
}

#[test]
fn prime_post_established_push_reject_errors_never_degrades() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    // Remote and store diverge: fetch-ff onto an ESTABLISHED store can't ff, so
    // the op aborts (E5) rather than silently degrading to stealth.
    let other = checkout(tmp.path(), &remote, "other");
    commit(&other, "remote.txt", "remote");
    git(&other, &["push", "-q", "origin", BRANCH]).unwrap();
    commit(&store, "local.txt", "local");
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    assert!(prime_post(&binding(Some(&remote), &store), &env).is_err());
}

#[test]
fn prime_post_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let store = local_unpushed(tmp.path());
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    prime_post(&binding(None, &store), &env).unwrap(); // no remote → nothing
}

#[test]
fn store_elsewhere_reports_a_differently_configured_origin() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_config(tmp.path(), "balls/work");
    let landing = local_unpushed(tmp.path());
    let b = default_binding(Some(&remote), &landing);
    // The synced origin:balls/config names a different store than our default.
    assert_eq!(
        store_elsewhere(&b, &landing, b.remote.as_deref().unwrap()),
        Some("balls/work".to_string())
    );
}

#[test]
fn store_elsewhere_is_silent_when_origin_agrees_or_is_unreadable() {
    let tmp = TempDir::new().unwrap();
    let landing = local_unpushed(tmp.path());
    // Origin's config names the SAME default store → no gap.
    let agree = remote_with_config(tmp.path(), crate::DEFAULT_TASKS_BRANCH);
    let b = default_binding(Some(&agree), &landing);
    assert_eq!(store_elsewhere(&b, &landing, b.remote.as_deref().unwrap()), None);
    // No balls/config branch to read → uncatchable, silent.
    let bare = empty_remote(tmp.path());
    let b = default_binding(Some(&bare), &landing);
    assert_eq!(store_elsewhere(&b, &landing, b.remote.as_deref().unwrap()), None);
}

#[test]
fn prime_warns_about_a_relocated_store_but_does_not_abort() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_config(tmp.path(), "balls/work");
    let landing = local_unpushed(tmp.path());
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    // Default tasks_branch + a relocated origin: prime warns (stderr), then
    // clone-in finds no default `balls/tasks` on the remote and does nothing —
    // the warning is diagnostic, never fatal.
    let b = Binding { landing: landing.to_string_lossy().into_owned(), ..default_binding(Some(&remote), &landing) };
    prime(&b, &env).unwrap();
}
