//! End-to-end harness for the `tracker` plugin binary: build it and drive it as
//! balls would (`<bin> <op> <phase>` with the §7 payload on stdin), against a
//! throwaway bare remote — never the dev repo. The library unit tests cover the
//! handler branches; this proves the process boundary (argv, stdin, exit code).

use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;
use std::process::Command as Git;
use tempfile::TempDir;

/// Run a setup git command, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    assert!(Git::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success());
}

/// `cwd`'s tip of `rev`, trimmed.
fn tip(cwd: &Path, rev: &str) -> String {
    let out = Git::new("git").arg("-C").arg(cwd).args(["rev-parse", rev]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn protocol_self_describes_to_stdout() {
    Command::cargo_bin("tracker")
        .unwrap()
        .arg("protocol")
        .assert()
        .success()
        .stdout(contains("\"protocol\":1"))
        .stdout(contains("\"sync\""));
}

#[test]
fn a_deliverable_verbs_post_pushes_the_sealed_branch() {
    let tmp = TempDir::new().unwrap();
    let remote = tmp.path().join("remote.git");
    git(tmp.path(), &["init", "--bare", "-q", "-b", "balls", &remote.to_string_lossy()]);
    let op = tmp.path().join("op");
    git(tmp.path(), &["clone", "-q", &remote.to_string_lossy(), &op.to_string_lossy()]);
    git(&op, &["config", "user.email", "t@e"]);
    git(&op, &["config", "user.name", "t"]);
    std::fs::write(op.join("a.txt"), "a\n").unwrap();
    git(&op, &["add", "-A"]);
    git(&op, &["commit", "-q", "-m", "land"]);

    let payload = format!(
        r#"{{"binding":{{"remote":"{}","tasks_branch":"balls","store":"{}","landing":"{}","invocation_path":"{}"}}}}"#,
        remote.display(),
        op.display(),
        op.display(),
        op.display()
    );
    Command::cargo_bin("tracker")
        .unwrap()
        .args(["claim", "post"])
        .write_stdin(payload)
        .assert()
        .success();

    assert_eq!(tip(&remote, "balls"), tip(&op, "HEAD"));
}
