//! §1/§6 the unified op log + the best-effort read-op fold: a mutating op
//! envelopes each plugin-stderr line as a `{src,lvl,op,phase,msg}` record,
//! truncates an oversized line with a marker, and lands the failure locus at
//! `error` (surviving the threshold); a failing READ-op plugin still renders.

use serde_json::Value;

use crate::harness::setup;

/// The first log record satisfying `pred`.
fn find(recs: &[Value], pred: impl Fn(&Value) -> bool) -> Option<&Value> {
    recs.iter().find(|r| pred(r))
}

#[test]
fn a_mutating_op_envelopes_stderr_truncates_and_lands_the_error() {
    // `talker` on create.pre writes a normal stderr line, then a >LINE_MAX blob
    // with no newline, then exits non-zero. The relay envelopes both lines as
    // records; the oversized one is truncated with the marker; the non-zero exit
    // lands an `error` record that outranks the default `info` threshold.
    let e = setup();
    let body = "cat >/dev/null\n\
        echo 'hello from talker' 1>&2\n\
        awk 'BEGIN{for(i=0;i<5000;i++)printf \"x\"}' 1>&2\n\
        exit 1";
    let path = e.write_plugin("talker", body);
    e.bind("talker", &path);
    e.ok(&["conf", "append", "create.pre", "talker"]);

    let out = e.bl(&["create", "noisy", "--as", "me"]);
    assert!(!out.status.success(), "talker aborts the op");
    let recs = e.log_records();

    // The enveloped stderr record carries every axis: src=plugin, lvl=info,
    // op=create, phase=pre (a mutating envelope is phase-tagged).
    let env = find(&recs, |r| r["src"] == "talker" && r["msg"] == "hello from talker")
        .expect("the plugin's stderr was enveloped");
    assert_eq!(env["lvl"], "info");
    assert_eq!(env["op"], "create");
    assert_eq!(env["phase"], "pre");

    // The oversized line is truncated with the lossy marker so the record still
    // fits the atomic-append bound (§1).
    assert!(
        recs.iter().any(|r| r["src"] == "talker" && r["msg"].as_str().is_some_and(|m| m.contains("…[truncated]"))),
        "the huge stderr line is truncated with the marker",
    );

    // The non-zero exit is the failure locus: an `error` record from core naming
    // the aborting plugin, surviving the default threshold.
    let err = find(&recs, |r| r["lvl"] == "error" && r["src"] == "core" && r["op"] == "create")
        .expect("the abort landed an error record");
    assert!(err["msg"].as_str().unwrap().contains("talker aborted the op"), "names the locus: {err}");
}

#[test]
fn a_failing_read_op_plugin_still_renders_and_lands_an_error() {
    // §6 read dispatch is best-effort: a plugin wired under the bare `list` key
    // that exits non-zero drops only ITS folded line — the read still renders the
    // store — but the failure locus lands at `error` in the op log.
    let e = setup();
    e.ok(&["create", "survivor", "--as", "me"]);
    let path = e.write_plugin("rdfail", "cat >/dev/null\nexit 1");
    e.bind("rdfail", &path);
    e.ok(&["conf", "append", "list", "rdfail"]);

    // The read succeeds and still shows the ball despite the failed fold.
    let listed = e.ok(&["list"]);
    assert!(listed.contains("survivor"), "the read still renders the store: {listed}");

    // The read-op failure lands an error record (no pre/post split — a read is
    // one phase, so the record carries no `phase` field).
    let recs = e.log_records();
    let err = find(&recs, |r| {
        r["lvl"] == "error" && r["op"] == "list" && r["msg"].as_str().is_some_and(|m| m.contains("rdfail"))
    })
    .expect("the failed read plugin lands an error record");
    assert!(err["msg"].as_str().unwrap().contains("failed the list read dispatch"), "{err}");
    assert!(err.get("phase").is_none(), "a read record has no phase axis: {err}");
}
