//! E2E for `bl-speculate` (bl-1263): spawn the real binary in a throwaway
//! repo with isolated XDG state and prove the exit-code contract the hook
//! relies on — miss → record → hit, a recorded fail is not a pass, usage and
//! environment errors. tarpaulin counts src/ only, so this file is
//! coverage-neutral; the llvm engine attributes the spawned binary's src
//! lines here (the adversary-plugin pattern).

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Sys;

use assert_cmd::Command;
use tempfile::TempDir;

/// A throwaway repo carrying the gate files, plus an isolated XDG home.
struct Env {
    _tmp: TempDir,
    repo: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

fn env() -> Env {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("scripts")).unwrap();
    for rel in ["scripts/pre-commit", "scripts/check-line-lengths.sh", "scripts/check-coverage.sh"] {
        fs::write(repo.join(rel), format!("#!/bin/sh\n# {rel}\n")).unwrap();
    }
    fs::write(repo.join("Makefile"), "all:\n").unwrap();
    fs::write(repo.join("code.rs"), "fn main() {}\n").unwrap();
    assert!(Sys::new("git").arg("-C").arg(&repo).arg("init").arg("-q").status().unwrap().success());
    let home = tmp.path().join("home");
    let state = tmp.path().join("state");
    fs::create_dir_all(&home).unwrap();
    Env { _tmp: tmp, repo, home, state }
}

/// The binary under test, rooted in the fixture's repo and XDG isolation.
fn speculate(e: &Env) -> Command {
    let mut cmd = Command::cargo_bin("bl-speculate").unwrap();
    cmd.current_dir(&e.repo)
        .env("HOME", &e.home)
        .env("XDG_STATE_HOME", &e.state)
        .env_remove("XDG_CONFIG_HOME")
        .env("BALLS_IDENTITY", "e2e")
        .env("BALLS_TOOLCHAIN", "rustc 1.0 (e2e)");
    cmd
}

#[test]
fn miss_record_hit_is_the_lifecycle() {
    let e = env();
    speculate(&e).arg("check").assert().code(3);
    speculate(&e).arg("record").arg("pass").assert().success();
    speculate(&e).arg("check").assert().success();
    fs::write(e.repo.join("code.rs"), "fn main() { let _ = 1; }\n").unwrap();
    speculate(&e).arg("check").assert().code(3);
}

#[test]
fn a_recorded_fail_is_not_a_pass() {
    let e = env();
    speculate(&e).arg("record").arg("fail").assert().success();
    speculate(&e).arg("check").assert().code(3);
    let verdicts = e.state.join("balls/plugins/bl-speculate/verdicts");
    assert_eq!(fs::read_dir(&verdicts).unwrap().count(), 1, "the fail IS recorded");
}

#[test]
fn usage_errors_exit_one_with_a_voice() {
    let e = env();
    for args in [vec![], vec!["record"], vec!["record", "maybe"], vec!["bogus"]] {
        let mut cmd = speculate(&e);
        for a in &args {
            cmd.arg(a);
        }
        let out = cmd.assert().code(1).get_output().clone();
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: bl-speculate"),
            "argv {args:?} must speak usage"
        );
    }
}

#[test]
fn unset_home_is_an_error_not_a_panic() {
    let e = env();
    let mut cmd = speculate(&e);
    cmd.env_remove("HOME").arg("check").assert().code(1);
}

#[test]
fn unset_toolchain_fails_open_as_an_error_naming_the_variable() {
    let e = env();
    for blank in [None, Some(""), Some("  \n")] {
        let mut cmd = speculate(&e);
        match blank {
            None => cmd.env_remove("BALLS_TOOLCHAIN"),
            Some(v) => cmd.env("BALLS_TOOLCHAIN", v),
        };
        let out = cmd.arg("check").assert().code(1).get_output().clone();
        assert!(String::from_utf8_lossy(&out.stderr).contains("BALLS_TOOLCHAIN"), "{blank:?} must name the variable");
    }
}

/// The key is what the 0.5.12 shell-out derived: git's hash of the trimmed
/// `rustc -V` line. A gate exporting `"$(rustc -V)"` — newline or not — keeps
/// every verdict recorded before the gate owned its fingerprint (ops bl-6124).
#[test]
fn the_gate_half_is_the_git_hash_of_the_trimmed_toolchain() {
    let e = env();
    let line = "rustc 1.95.0 (59807616e 2026-04-14)";
    speculate(&e).env("BALLS_TOOLCHAIN", format!("{line}\n")).arg("record").arg("pass").assert().success();
    speculate(&e).env("BALLS_TOOLCHAIN", line).arg("check").assert().success();
    let mut hash = Sys::new("git");
    hash.arg("-C").arg(&e.repo).args(["hash-object", "--stdin"]).stdin(std::process::Stdio::piped());
    let mut child = hash.stdout(std::process::Stdio::piped()).spawn().unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), line.as_bytes()).unwrap();
    let expected = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap().trim().to_string();
    let verdicts = e.state.join("balls/plugins/bl-speculate/verdicts");
    let names: Vec<String> =
        fs::read_dir(&verdicts).unwrap().map(|d| d.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 1, "one key for both spellings: {names:?}");
    assert!(names[0].ends_with(&format!("-{expected}.toml")), "{} vs {expected}", names[0]);
}

#[test]
fn import_adopts_foreign_verdicts_by_file() {
    let e = env();
    let tree = "a".repeat(40);
    let gate = "b".repeat(40);
    let foreign = e.home.join(format!("{tree}-{gate}.toml"));
    fs::write(&foreign, "pass = true\nbuilder = \"github-actions\"\n").unwrap();
    let out = speculate(&e).arg("import").arg(&foreign).assert().success();
    let spoken = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(spoken.contains(&format!("imported {tree} {gate}")), "{spoken}");
    let adopted = e.state.join("balls/plugins/bl-speculate/verdicts").join(format!("{tree}-{gate}.toml"));
    let body = fs::read_to_string(&adopted).unwrap();
    assert!(body.contains("github-actions"), "the builder identity crossed: {body}");
    speculate(&e).arg("import").assert().code(1);
    let bogus = e.home.join("bogus.toml");
    fs::write(&bogus, "pass = true\nbuilder = \"x\"\n").unwrap();
    speculate(&e).arg("import").arg(&bogus).assert().code(1);
}

#[test]
fn territory_lands_under_the_plugin_namespace() {
    let e = env();
    speculate(&e).arg("record").arg("pass").assert().success();
    let verdicts = e.state.join("balls/plugins/bl-speculate/verdicts");
    let entries: Vec<_> = fs::read_dir(&verdicts).unwrap().map(|d| d.unwrap().file_name()).collect();
    assert_eq!(entries.len(), 1);
    let name = entries[0].to_string_lossy().into_owned();
    let ext = Path::new(&name).extension().map(std::ffi::OsStr::to_owned);
    assert_eq!(ext.as_deref(), Some("toml".as_ref()), "one verdict file per (tree, gate): {name}");
    let body = fs::read_to_string(Path::new(&verdicts).join(&name)).unwrap();
    assert!(body.contains("builder = \"e2e\""), "builder identity is recorded: {body}");
}

#[test]
fn the_version_line_names_this_binary_and_needs_no_repo() {
    // A sibling states its OWN version (bl-4316), answered before the cwd is
    // read — `bl-speculate` is reached from a hook, but the box's reconciler
    // asks it from wherever it happens to be standing.
    for spelling in ["--version", "-V"] {
        let out = Command::cargo_bin("bl-speculate")
            .unwrap()
            .current_dir(std::env::temp_dir())
            .arg(spelling)
            .assert()
            .success();
        let spoken = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
        assert!(spoken.starts_with("bl-speculate "), "{spoken}");
    }
}
