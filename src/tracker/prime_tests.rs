//! Tests for the §12 prime handlers — the stealth no-op, `prime/pre`
//! clone-in (established/present/bootstrap), `prime/post` founding vs publish
//! (and their rejection paths), and the store-elsewhere diagnostic.

use super::*;
use crate::tracker::fixtures::{
    binding, checkout, commit, default_binding, empty_remote, env, landing_repo, legacy_remote,
    local_unpushed, remote_with_branch, remote_with_config, store_clone, tip, tracked,
    unpushable_remote, BRANCH,
};
use std::fs;
use tempfile::TempDir;

#[test]
fn a_no_remote_prime_is_silent_and_persists_nothing() {
    // bl-9df0: stealth leaves NO tracker-side state (the lock file is gone) —
    // a DECLARED opt-out lives in core's config, an inferred one re-derives.
    // bl-2013: it is also SILENT (the routine W1 line is gone) — stealth is the
    // expected first-run, so narrating it every op was the wart.
    let tmp = TempDir::new().unwrap();
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    let b = binding(None, &tmp.path().join("landing"));
    prime(&b, &env).unwrap();
    assert!(!env.xdg.clone_dir(Path::new(&b.invocation_path)).root().exists());
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
fn prime_pre_quarantines_a_legacy_remote_store_instead_of_adopting_it() {
    // bl-868d: a shared hub still carrying the PRE-greenfield `balls/tasks`
    // (`.balls/` JSON, no `tasks/`) is NOT an established store — adopting it
    // wedged every fresh clone (the store checkout had no `tasks/`, the op
    // aborted, and re-prime hit the same abort). The §12 adopt-vs-found signal
    // is "an established STORE", read from the tip's shape: a no-`tasks/` tip
    // is left un-adopted (warn only) so core founds a fresh greenfield orphan,
    // exactly the runbook's "prime founds, import fills, cutover rewrites".
    let tmp = TempDir::new().unwrap();
    let remote = legacy_remote(tmp.path());
    let landing = landing_repo(tmp.path());
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    prime(&tracked(&remote, &landing, &landing), &env).unwrap();
    assert!(!local_branch(&landing, BRANCH)); // un-adopted — core founds instead
}

#[test]
fn prime_post_over_a_legacy_remote_converges_without_touching_it() {
    // The composed §12 content settle (sync + publish) over an un-cut-over hub:
    // both halves see "not a store yet" and keep the work local — no abort, no
    // stealth degrade, the legacy ref untouched (cutover is the runbook's
    // explicit history join + fast-forward push, never an implicit overwrite).
    let tmp = TempDir::new().unwrap();
    let remote = legacy_remote(tmp.path());
    let store = local_unpushed(tmp.path()); // the freshly-founded greenfield store
    let before = tip(&remote, BRANCH);
    prime_post(&binding(Some(&remote), &store)).unwrap();
    assert_eq!(tip(&remote, BRANCH), before); // the legacy ref was not rewritten
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
    prime_post(&binding(Some(&remote), &store)).unwrap();
    assert_eq!(tip(&remote, BRANCH), tip(&store, "HEAD")); // the push created the branch
}

#[test]
fn prime_post_brings_current_then_publishes_to_an_established_remote() {
    let tmp = TempDir::new().unwrap();
    let remote = remote_with_branch(tmp.path());
    let store = store_clone(tmp.path(), &remote);
    let landed = commit(&store, "landed.txt", "landed"); // local work ahead of the remote
    prime_post(&binding(Some(&remote), &store)).unwrap();
    assert_eq!(tip(&remote, BRANCH), landed); // fetch-ff is a no-op, the push publishes
}

#[test]
fn prime_post_founding_reject_degrades_silently_and_persists_nothing() {
    // bl-9df0: the founding-miss is an OUTCOME, not a consent — nothing is
    // written, so §12's "re-running prime re-attempts" holds by construction.
    let tmp = TempDir::new().unwrap();
    let remote = unpushable_remote(tmp.path());
    let store = local_unpushed(tmp.path());
    prime_post(&binding(Some(&remote), &store)).unwrap(); // denied founding push → silent
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
    assert!(prime_post(&binding(Some(&remote), &store)).is_err());
}

#[test]
fn prime_post_in_stealth_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let store = local_unpushed(tmp.path());
    prime_post(&binding(None, &store)).unwrap(); // no remote → nothing
}

#[test]
fn the_ephemeral_gap_compares_the_acting_remote_to_the_durable_ladder() {
    // W2 (bl-c2de): warn iff the remote prime acts on is NOT what the durable
    // ladder (landing `task_remote` > per-clone binding > legacy XDG > `origin`)
    // resolves — a per-op `--remote` that
    // plain commands will silently not reproduce (the bl-d234 failure).
    let tmp = TempDir::new().unwrap();
    let env = env(&tmp.path().join("home"), &tmp.path().join("state"));
    let repo = landing_repo(tmp.path()); // no origin remote
    let b = binding(None, &repo); // invocation_path = repo
    // No durable tier at all → the gap names stealth.
    assert_eq!(
        ephemeral_gap(&b, &env, "git@hub:r").as_deref(),
        Some("nothing (plain commands run stealth)")
    );
    // An `origin` matching the acting remote → no gap (the flag was redundant).
    git(&repo, &["remote", "add", "origin", "git@hub:r"]).unwrap();
    assert_eq!(ephemeral_gap(&b, &env, "git@hub:r"), None);
    // The XDG tier outranks origin; a DIFFERENT durable remote is the gap.
    let cfg = env.xdg.user_config();
    fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    fs::write(&cfg, "remote = \"git@hub:durable\"\n").unwrap();
    assert_eq!(ephemeral_gap(&b, &env, "git@hub:r").as_deref(), Some("`git@hub:durable`"));
    assert_eq!(ephemeral_gap(&b, &env, "git@hub:durable"), None);
    // The per-clone binding remote outranks the legacy XDG one (bl-d081): a remote
    // core read from this clone's binding is NOT misreported as ephemeral.
    let binding_path = env.xdg.clone_dir(std::path::Path::new(&b.invocation_path)).binding();
    fs::create_dir_all(binding_path.parent().unwrap()).unwrap();
    fs::write(&binding_path, "remote = \"git@hub:bind\"\n").unwrap();
    assert_eq!(ephemeral_gap(&b, &env, "git@hub:r").as_deref(), Some("`git@hub:bind`"));
    assert_eq!(ephemeral_gap(&b, &env, "git@hub:bind"), None);
    // A landing stealth sentinel outranks everything durable: plain commands
    // run DECLARED stealth, named as such (bl-9df0).
    let stealth_landing = tmp.path().join("stealth-landing");
    fs::create_dir_all(stealth_landing.join("config")).unwrap();
    fs::write(stealth_landing.join("config").join("balls.toml"), "task_remote = \"none\"\n").unwrap();
    let declared = Binding { landing: stealth_landing.to_string_lossy().into_owned(), ..b };
    assert_eq!(
        ephemeral_gap(&declared, &env, "git@hub:r").as_deref(),
        Some("declared stealth (the landing `task_remote` sentinel)")
    );
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
