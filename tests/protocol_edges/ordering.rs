//! §8/§14 ordering: pre plugins run in hook-list order, and on an abort the
//! engine rolls the prior plugins back in strict REVERSE, each rollback carrying
//! the §7 `rolling_back` phase tag on its wire.

use std::fs;

use crate::harness::setup;

/// The lines a stamp-plugin marker accumulated, in order.
fn marker_lines(path: &std::path::Path) -> Vec<String> {
    fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect()
}

#[test]
fn two_plugins_execute_in_configured_list_order() {
    // list position = run order (§6): append `first` then `second`, and the
    // forward stamps land in exactly that order.
    let e = setup();
    let marker = e.project.join("order.log");
    e.stamp_plugin("first", &marker, 0);
    e.stamp_plugin("second", &marker, 0);
    e.ok(&["conf", "append", "create.pre", "first"]);
    e.ok(&["conf", "append", "create.pre", "second"]);

    e.ok(&["create", "ordered", "--as", "me"]);
    assert_eq!(marker_lines(&marker), ["FWD first", "FWD second"], "run in configured order");
}

#[test]
fn a_mid_chain_failure_rolls_the_prior_plugins_back_in_reverse_with_the_tag() {
    // A-succeeds / B-succeeds / C-fails on create.pre: the forward chain records
    // A, B, C; then §14 unwinds the RUN plugins (A, B — C failed, so it never
    // recorded) in strict reverse (B then A), and each rollback call carries the
    // undone phase as the §7 `rolling_back` tag on its wire.
    let e = setup();
    let marker = e.project.join("rollback.log");
    e.stamp_plugin("alpha", &marker, 0);
    e.stamp_plugin("beta", &marker, 0);
    e.stamp_plugin("gamma", &marker, 1); // the aborter
    for name in ["alpha", "beta", "gamma"] {
        e.ok(&["conf", "append", "create.pre", name]);
    }

    let out = e.bl(&["create", "unwind me", "--as", "me"]);
    assert!(!out.status.success(), "gamma aborts the op");

    let lines = marker_lines(&marker);
    assert_eq!(
        lines,
        ["FWD alpha", "FWD beta", "FWD gamma", "RB beta pre", "RB alpha pre"],
        "forward in order, then rollback in reverse carrying rolling_back=pre",
    );
    // gamma failed forward, so it is NOT recorded as run and never rolls back.
    assert!(!lines.iter().any(|l| l.starts_with("RB gamma")), "the aborter is not rolled back");
}
