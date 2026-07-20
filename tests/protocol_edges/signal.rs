//! §6/§14 plugin DEATH-BY-SIGNAL vs a clean non-zero exit: a plugin the kernel
//! kills mid-hook (its `ExitStatus` carries NO exit code) must still abort the
//! op LOUDLY — the abort record renders the signal death sanely, not garbled —
//! roll the prior plugins back in reverse, and land a coherent `error` record.
//! The clean `exit 1` sibling proves the two render distinctly (`signal:` vs
//! `exit status`), so neither branch is a garbled empty tail.

use std::fs;

use serde_json::Value;

use crate::harness::setup;

/// The lines a stamp-plugin marker accumulated, in order.
fn marker_lines(path: &std::path::Path) -> Vec<String> {
    fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect()
}

/// The core `error` record for the `create` op, if one landed.
fn abort_error(recs: &[Value]) -> Option<&Value> {
    recs.iter().find(|r| r["lvl"] == "error" && r["src"] == "core" && r["op"] == "create")
}

#[test]
fn a_signal_killed_plugin_aborts_loudly_and_rolls_the_survivor_back() {
    // `early` (stamp, exits 0) then `killer` on create.pre: killer drains the
    // wire, writes a partial stderr line, then `kill -9 $$` — the kernel reaps
    // it, so its ExitStatus has no exit code. The op must still abort: killer
    // never succeeds, so §14 unwinds only the RUN plugin (early) in reverse, and
    // the abort record renders the signal death sanely (a `signal:` locus, never
    // a garbled empty-code tail).
    let e = setup();
    let marker = e.project.join("signal.log");
    e.stamp_plugin("early", &marker, 0);
    let killer = e.write_plugin("killer", "cat >/dev/null\necho 'partial from killer' 1>&2\nkill -9 $$");
    e.bind("killer", &killer);
    e.ok(&["conf", "append", "create.pre", "early"]);
    e.ok(&["conf", "append", "create.pre", "killer"]);

    let out = e.bl(&["create", "reaped", "--as", "me"]);
    assert!(!out.status.success(), "a signal-killed plugin still aborts the op");

    // early ran and rolled back; killer never succeeded, so it is NOT unwound.
    assert_eq!(marker_lines(&marker), ["FWD early", "RB early pre"], "the survivor rolls back in reverse");

    let recs = e.log_records();
    let err = abort_error(&recs).expect("the signal death landed an error record");
    let msg = err["msg"].as_str().unwrap();
    assert!(msg.contains("killer aborted the op"), "names the locus, not garbled: {msg}");
    // FINDING (works-as-designed): the abort embeds `ExitStatus`'s Display, which
    // on a signal death renders `signal: N (SIGKILL)` — sane, never an empty
    // `()` code tail. Pinned here so a future formatting regression is loud.
    assert!(msg.contains("signal"), "renders the signal death sanely (signal: N): {msg}");
    assert_eq!(err["phase"], "pre", "the abort record carries the create.pre phase: {err}");
    // The partial stderr written before the kill was still enveloped by name.
    assert!(
        recs.iter().any(|r| r["src"] == "killer" && r["msg"] == "partial from killer"),
        "the partial stderr written before the kill was enveloped",
    );
}

#[test]
fn a_clean_exit_one_renders_an_exit_status_not_a_signal() {
    // The contrast: a plugin that writes the same partial stderr then `exit 1`
    // aborts identically, but its abort record names an `exit status`, proving
    // the signal branch above is a distinct, sane render — not the same string.
    let e = setup();
    let quitter = e.write_plugin("quitter", "cat >/dev/null\necho 'partial from quitter' 1>&2\nexit 1");
    e.bind("quitter", &quitter);
    e.ok(&["conf", "append", "create.pre", "quitter"]);

    let out = e.bl(&["create", "quit", "--as", "me"]);
    assert!(!out.status.success(), "a clean exit 1 aborts the op");

    let recs = e.log_records();
    let msg =
        abort_error(&recs).expect("the exit-1 abort landed an error record")["msg"].as_str().unwrap().to_string();
    assert!(msg.contains("quitter aborted the op"), "names the locus: {msg}");
    assert!(msg.contains("exit status"), "a clean exit renders an exit status, not a signal: {msg}");
    assert!(!msg.contains("signal"), "a clean exit is not a signal death: {msg}");
}
