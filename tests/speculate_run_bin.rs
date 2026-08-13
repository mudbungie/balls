//! E2E for `bl-speculate run` (bl-d0c2): the eagerness ladder and flag
//! parsing live in the binary edge, so they are proven here, spawning the
//! real binary with a stub gate. Coverage-neutral file; the llvm engine
//! attributes the spawned binary's src lines here.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as Sys;

use assert_cmd::Command;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) {
    let out = Sys::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

struct Env {
    tmp: TempDir,
    repo: PathBuf,
    gate: PathBuf,
}

/// A repo with gate files, `main`, and two clean work branches, plus a stub
/// gate and an isolated XDG home.
fn env() -> Env {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("scripts")).unwrap();
    for rel in ["scripts/pre-commit", "scripts/check-line-lengths.sh", "scripts/check-coverage.sh"] {
        fs::write(repo.join(rel), "#!/bin/sh\n").unwrap();
    }
    fs::write(repo.join("Makefile"), "all:\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("f"), "base\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    for (branch, file) in [("work/a", "fa"), ("work/b", "fb")] {
        git(&repo, &["checkout", "-q", "-b", branch]);
        fs::write(repo.join(file), branch).unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", branch]);
        git(&repo, &["checkout", "-q", "main"]);
    }
    let gate = tmp.path().join("gate.sh");
    fs::write(&gate, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&gate, fs::Permissions::from_mode(0o755)).unwrap();
    Env { tmp, repo, gate }
}

fn speculate(e: &Env) -> Command {
    let mut cmd = Command::cargo_bin("bl-speculate").unwrap();
    cmd.current_dir(&e.repo)
        .env("HOME", e.tmp.path().join("home"))
        .env("XDG_STATE_HOME", e.tmp.path().join("state"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_SPECULATE_EAGERNESS")
        .env_remove("BALLS_POWER_SYS");
    fs::create_dir_all(e.tmp.path().join("home")).unwrap();
    cmd
}

fn gate_arg(e: &Env) -> String {
    e.gate.to_string_lossy().into_owned()
}

#[test]
fn run_builds_the_queue_and_reports() {
    let e = env();
    speculate(&e).arg("enqueue").arg("a").assert().success();
    speculate(&e).arg("enqueue").arg("b").assert().success();
    let out = speculate(&e).args(["run", "--gate", &gate_arg(&e)]).assert().success();
    let report = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(report.contains("built a") && report.contains("built b"), "{report}");
}

#[test]
fn declared_eagerness_zero_is_off_and_gibberish_is_loud() {
    let e = env();
    speculate(&e).arg("enqueue").arg("a").assert().success();
    let out = speculate(&e)
        .args(["run", "--gate", &gate_arg(&e)])
        .env("BALLS_SPECULATE_EAGERNESS", "0")
        .assert()
        .success();
    let report = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(report.contains("deferred a"), "declared 0 spends nothing: {report}");
    speculate(&e)
        .args(["run", "--gate", &gate_arg(&e)])
        .env("BALLS_SPECULATE_EAGERNESS", "lots")
        .assert()
        .code(1);
}

#[test]
fn battery_evidence_throttles_to_one_build_per_pass() {
    let e = env();
    let sys = e.tmp.path().join("sys");
    fs::create_dir_all(sys.join("AC")).unwrap();
    fs::write(sys.join("AC/online"), "0\n").unwrap();
    speculate(&e).arg("enqueue").arg("a").assert().success();
    speculate(&e).arg("enqueue").arg("b").assert().success();
    let out = speculate(&e)
        .args(["run", "--gate", &gate_arg(&e)])
        .env("BALLS_POWER_SYS", &sys)
        .assert()
        .success();
    let report = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(report.contains("built a") && report.contains("deferred b"), "{report}");
    fs::write(sys.join("AC/online"), "1\n").unwrap();
    let out = speculate(&e)
        .args(["run", "--gate", &gate_arg(&e)])
        .env("BALLS_POWER_SYS", &sys)
        .assert()
        .success();
    let report = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(report.contains("hit a") && report.contains("built b"), "AC builds on: {report}");
}

#[test]
fn run_flag_abuse_speaks_usage() {
    let e = env();
    for args in [
        vec!["run", "--gate"],
        vec!["run", "--bogus", "x"],
        vec!["run", "--builds", "many"],
    ] {
        let out = speculate(&e).args(&args).assert().code(1).get_output().clone();
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: bl-speculate"),
            "{args:?} must speak usage"
        );
    }
}

#[test]
fn explicit_builds_beats_the_ladder() {
    let e = env();
    speculate(&e).arg("enqueue").arg("a").assert().success();
    let out = speculate(&e)
        .args(["run", "--gate", &gate_arg(&e), "--builds", "0"])
        .env("BALLS_SPECULATE_EAGERNESS", "5")
        .assert()
        .success();
    let report = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(report.contains("deferred a"), "explicit --builds wins: {report}");
}
