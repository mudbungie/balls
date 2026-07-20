//! End-to-end stress of the plugin-protocol defensive machinery (bl-939d):
//! the §6 recursion depth cap, the unbound-name refusal, §14 reverse rollback
//! with the `rolling_back` wire tag, list-order dispatch, the best-effort
//! read-op fold, and the enveloped op-log (src/lvl/op/phase + truncation).
//!
//! Every scenario drives the real `bl` against fake shell-script plugins in an
//! isolated tempdir (see [`harness`]); the assertions are the OBSERVABLE
//! outcomes — stderr/exit, a marker file the plugins stamp, and the JSON-lines
//! op log — never internals. tarpaulin counts src/ only, so this is
//! coverage-neutral.

#![cfg(unix)]

mod harness;
mod logging;
mod ordering;

use predicates::str::contains;

use harness::setup;

#[test]
fn the_depth_cap_aborts_loudly_naming_op_phase_and_plugin() {
    // §6/bl-7110: a plugin wired at the invocation-tree cap ABORTS the op — fail,
    // never a silent plugin-free run. The message names the op.phase that overran
    // AND the plugin it refused to spawn.
    let e = setup();
    let noop = e.write_plugin("capd", "cat >/dev/null\nexit 0");
    e.bind("capd", &noop);
    e.ok(&["conf", "append", "create.pre", "capd"]);

    let out = e.cmd(&["create", "runaway", "--as", "me"]).env("BALLS_PLUGIN_DEPTH", "8").output().unwrap();
    assert!(!out.status.success(), "the cap must abort the op");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("invocation-tree depth cap (8) reached at create.pre"), "names op.phase: {err}");
    assert!(err.contains("aborting before plugin capd"), "names the plugin, never plugin-free: {err}");
}

#[test]
fn an_unbound_name_with_no_source_hint_says_run_bl_install() {
    // §6: a hooked name whose `bin/<name>` is missing and which carries NO
    // `[source]` hint aborts the op pointing at the bind step — `run bl install`.
    let e = setup();
    e.ok(&["conf", "append", "create.pre", "phantom"]);

    e.cmd(&["create", "orphaned", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("plugin phantom referenced but bin/phantom missing — run bl install"));
}

#[test]
fn a_freshly_primed_claim_post_omits_bl_chore() {
    // The default seed schedule wires the two shipped capabilities on claim.post
    // (bl-delivery then bl-tracker) and NOTHING else — bl-chore is this repo's
    // own landing wiring, never a fresh-prime default.
    let e = setup();
    let claim_post = e.ok(&["conf", "claim.post"]);
    assert!(claim_post.contains("bl-delivery"), "the shipped worktree plugin is wired: {claim_post}");
    assert!(!claim_post.contains("bl-chore"), "no bl-chore in the default schedule: {claim_post}");
}
