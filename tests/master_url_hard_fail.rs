//! bl-dcd3 hard-fail tests: a `master_url` client whose hub is
//! unreachable must refuse first-time setup rather than silently
//! dropping to an unlinked local orphan that would drift away from
//! the rest of the team. Three sites cover the model:
//! - `bl prime`'s auto-provision (the documented onboarding command),
//! - `bl init` with a seeded master_url,
//! - `bl remaster --detach` as the offline escape hatch.
//!
//! Split out from `tests/master_url.rs` to keep both files under the
//! 300-line cap. The happy-path master_url conformance lives there;
//! the unreachable-hub edge cases live here.

mod common;

use common::*;
use std::fs;
use std::path::Path;

fn read_master_url(repo: &Path) -> Option<String> {
    let s = fs::read_to_string(repo.join(".balls/config.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get("master_url")?.as_str().map(String::from)
}

fn seed_unreachable_master_url(repo: &Path) -> String {
    let url = "/this/path/does/not/exist/hub.git".to_string();
    seed_config(repo, &[("master_url", &url)]);
    url
}

#[test]
fn bl_prime_with_unreachable_master_url_hard_fails() {
    let code = new_bare_remote();
    let onboard = clone_from_remote(code.path(), "onboard");
    init_in(onboard.path());
    let url = seed_unreachable_master_url(onboard.path());
    git(onboard.path(), &["add", ".balls/config.json"]);
    git(onboard.path(), &["commit", "-m", "wire master_url"]);
    git(onboard.path(), &["push", "origin", "main"]);

    let teammate = clone_from_remote(code.path(), "teammate");
    let assert = bl(teammate.path()).arg("prime").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("could not reach state hub"), "{stderr}");
    assert!(stderr.contains(&url), "stderr must name URL: {stderr}");
    assert!(stderr.contains("remaster --detach"), "{stderr}");
    assert!(
        !teammate.path().join(".balls/state-repo").exists(),
        "no partial state-repo must remain after hard-fail"
    );
}

#[test]
fn bl_init_with_unreachable_master_url_hard_fails() {
    let alice = new_repo();
    let url = seed_unreachable_master_url(alice.path());
    let assert = bl(alice.path()).arg("init").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("could not reach state hub") && stderr.contains(&url),
        "init stderr must surface hard-fail: {stderr}"
    );
}

#[test]
fn remaster_detach_works_offline_against_unreachable_master_url() {
    let alice = new_repo();
    seed_unreachable_master_url(alice.path());
    git(alice.path(), &["add", ".balls/config.json"]);
    git(alice.path(), &["commit", "-m", "wire master_url"]);

    bl(alice.path()).arg("remaster").arg("--detach").assert().success();
    assert!(read_master_url(alice.path()).is_none());
    assert!(
        alice.path().join(".balls/worktree/.balls/tasks").exists(),
        "detach must leave a usable legacy state worktree behind"
    );
    bl(alice.path()).arg("prime").assert().success();
    create_task(alice.path(), "post-detach task");
}
