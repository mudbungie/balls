//! Phase 1B-2 error and idempotency gates for `Store::init_xdg`.
//! Companion bundle to `conformance_xdg_init` (SPEC §14.1/4/15/19);
//! these tests cover the input-validation, missing-environment, and
//! warm-vs-cold tracker paths that the `bl init` implementation must
//! handle correctly even though the §14 gates don't observe them.

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::xdg_paths::own_tracker_checkout;
use common::tmp;
use common::xdg_init::{bases, fresh_clone_into, home_lock, init_xdg, HomeOverride};
use common::Repo;
use std::fs;

fn new_bare_remote() -> Repo {
    common::new_bare_remote()
}

#[test]
fn init_xdg_relative_tasks_dir_errors() {
    let home = tmp();
    let cwd = home.path().join("stealth");
    fs::create_dir_all(&cwd).unwrap();
    let _h = HomeOverride::new(home.path());
    let Err(err) = balls::Store::init_xdg(&cwd, true, Some("relative/path".into())) else {
        panic!("expected error for relative tasks_dir");
    };
    assert!(
        err.to_string().contains("must be an absolute path"),
        "got: {err}"
    );
}

#[test]
fn init_xdg_no_home_errors() {
    let home = tmp();
    let cwd = home.path().join("stealth");
    fs::create_dir_all(&cwd).unwrap();
    let guard = home_lock();
    let prior = std::env::var_os("HOME");
    unsafe { std::env::remove_var("HOME") };
    let res = balls::Store::init_xdg(&cwd, true, None);
    if let Some(v) = prior {
        unsafe { std::env::set_var("HOME", v) };
    }
    drop(guard);
    let Err(err) = res else {
        panic!("expected error for missing HOME");
    };
    assert!(err.to_string().contains("HOME must be set"), "got: {err}");
}

#[test]
fn init_xdg_no_origin_errors() {
    let home = tmp();
    let clone_root = home.path().join("no-origin");
    fs::create_dir_all(&clone_root).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&clone_root)
        .output()
        .expect("git init");
    let clone_root = fs::canonicalize(&clone_root).unwrap();
    let _h = HomeOverride::new(home.path());
    let Err(err) = balls::Store::init_xdg(&clone_root, false, None) else {
        panic!("expected error for missing origin");
    };
    assert!(err.to_string().contains("no origin configured"), "got: {err}");
}

#[test]
fn init_xdg_idempotent_warm_path() {
    let home = tmp();
    let remote = new_bare_remote();
    let origin_url = remote.path().to_string_lossy().into_owned();
    let clone = fresh_clone_into(home.path(), "dev/proj", &origin_url, "alice");

    init_xdg(&clone, home.path(), false, None);
    // Second call hits the warm path inside materialize_tracker.
    init_xdg(&clone, home.path(), false, None);

    let bases = bases(home.path());
    let enc_origin = percent_encode_component(&canonicalize_origin(&origin_url));
    let own = own_tracker_checkout(&bases, &enc_origin);
    assert!(own.join(".git").exists());
    assert!(own.join(".balls/repo.json").exists());
}

#[test]
fn init_xdg_second_clone_picks_up_existing_branch() {
    // First clone seeds origin with balls/tasks; second clone (sharing
    // the same origin, after we delete the local tracker dir) cold-
    // inits and must take the create_tracking_branch path because
    // origin already has the branch.
    let home = tmp();
    let remote = new_bare_remote();
    let origin_url = remote.path().to_string_lossy().into_owned();
    let clone_a = fresh_clone_into(home.path(), "dev/projA", &origin_url, "alice");
    init_xdg(&clone_a, home.path(), false, None);

    let bases = bases(home.path());
    let enc_origin = percent_encode_component(&canonicalize_origin(&origin_url));
    let tracker = own_tracker_checkout(&bases, &enc_origin);
    fs::remove_dir_all(&tracker).unwrap();

    let clone_b = fresh_clone_into(home.path(), "dev/projB", &origin_url, "bob");
    init_xdg(&clone_b, home.path(), false, None);

    assert!(tracker.join(".git").exists());
    assert!(tracker.join(".balls/repo.json").exists());
}
