//! End-to-end harness for the `bl-chore` plugin binary: drive it as balls would
//! (`<bin> <op> <phase>` with the §7 wire on stdin) and prove the process
//! boundary — argv, stdin, the protocol self-describe, and the exit code. The
//! library unit tests cover the guard/mint branches against a fake `bl`; this
//! covers the thin edge (the `main`/`run` shell tarpaulin sees via the built bin).

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn protocol_self_describes_to_stdout() {
    Command::cargo_bin("bl-chore")
        .unwrap()
        .arg("protocol")
        .assert()
        .success()
        .stdout(contains("\"protocol\":[1]"))
        .stdout(contains("\"ops\":[\"claim\"]"));
}

#[test]
fn tag_skip_is_a_clean_no_op_without_shelling_bl() {
    // A claim.post of a task already carrying the bl-chore tag bails before any
    // `bl` call — exit 0 — exercising argv + stdin + the dispatch wiring.
    let wire = r#"{"binding":{"landing":"/x","invocation_path":"/x"},"previous_state":{"tags":["bl-chore"]}}"#;
    Command::cargo_bin("bl-chore")
        .unwrap()
        .args(["claim", "post"])
        .env("BALLS_PLUGIN_NAME", "bl-chore")
        .write_stdin(wire)
        .assert()
        .success();
}

#[test]
fn missing_op_and_phase_are_usage_errors() {
    Command::cargo_bin("bl-chore").unwrap().assert().failure().stderr(contains("usage"));
    Command::cargo_bin("bl-chore").unwrap().args(["claim"]).assert().failure().stderr(contains("usage"));
}

#[test]
fn a_malformed_wire_aborts_the_claim() {
    // No BALLS_PLUGIN_NAME: covers the edge's default-name fallback line (the
    // value is unused here — the wire fails to parse before the name is read).
    Command::cargo_bin("bl-chore")
        .unwrap()
        .args(["claim", "post"])
        .env_remove("BALLS_PLUGIN_NAME")
        .write_stdin("not json")
        .assert()
        .failure()
        .stderr(contains("bl-chore:"));
}
