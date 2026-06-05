//! End-to-end harness for the default gating plugin: run the built `gate`
//! binary the way `bl` would (`<bin> <op> <phase>`, §7 payload on stdin, cwd =
//! the change worktree) and assert its allow/block exit code.

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use tempfile::TempDir;

/// Write a `tasks/<id>.md` under `dir` so the gate sees it as unresolved.
fn touch_task(dir: &std::path::Path, id: &str) {
    let tasks = dir.join("tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    std::fs::write(tasks.join(format!("{id}.md")), "+++\ntitle = \"t\"\ncreated = 0\nupdated = 0\n+++\n").unwrap();
}

fn run(ws: &TempDir, op: &str, payload: &str) -> i32 {
    Command::new(cargo_bin("gate"))
        .args([op, "pre"])
        .current_dir(ws.path())
        .write_stdin(payload)
        .assert()
        .get_output()
        .status
        .code()
        .unwrap()
}

#[test]
fn protocol_output_parses_as_a_plugin_self_description() {
    // Cross-check: the real loader can read what the binary prints (§6).
    let p = balls::plugin::describe(&cargo_bin("gate")).unwrap();
    assert!(p.speaks(balls::message::PROTOCOL));
    assert!(p.handles(balls::verb::Verb::Claim));
    assert!(p.handles(balls::verb::Verb::Close));
}

#[test]
fn claim_is_blocked_by_an_open_dependency() {
    let ws = TempDir::new().unwrap();
    touch_task(ws.path(), "bl-dep");
    let payload = r#"{"current_state":{"blockers":[{"id":"bl-dep","on":"claim"}]}}"#;
    assert_eq!(run(&ws, "claim", payload), 1);
}

#[test]
fn claim_is_allowed_once_the_dependency_resolves() {
    let ws = TempDir::new().unwrap();
    // bl-dep's file absent ⇒ resolved.
    let payload = r#"{"current_state":{"blockers":[{"id":"bl-dep","on":"claim"}]}}"#;
    assert_eq!(run(&ws, "claim", payload), 0);
}
