//! Integration tests for `bl completions` (generate, install, uninstall).

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn bl() -> Command {
    Command::cargo_bin("bl").unwrap()
}

fn bash_path(home: &std::path::Path) -> PathBuf {
    home.join(".local/share/bash-completion/completions/bl")
}
fn zsh_path(home: &std::path::Path) -> PathBuf {
    home.join(".local/share/zsh/site-functions/_bl")
}
fn fish_path(home: &std::path::Path) -> PathBuf {
    home.join(".local/share/fish/vendor_completions.d/bl.fish")
}

#[test]
fn generate_bash_completions_to_stdout() {
    bl()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn generate_zsh_and_fish_to_stdout() {
    bl().args(["completions", "zsh"]).assert().success();
    bl().args(["completions", "fish"]).assert().success();
}

#[test]
fn install_writes_three_files_under_home() {
    let home = tempfile::tempdir().unwrap();
    bl()
        .args(["completions", "--install"])
        .env("HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("installed"));
    assert!(bash_path(home.path()).exists());
    assert!(zsh_path(home.path()).exists());
    assert!(fish_path(home.path()).exists());
}

#[test]
fn uninstall_removes_files_written_by_install() {
    let home = tempfile::tempdir().unwrap();
    bl()
        .args(["completions", "--install"])
        .env("HOME", home.path())
        .assert()
        .success();
    bl()
        .args(["completions", "--uninstall"])
        .env("HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("removed"));
    assert!(!bash_path(home.path()).exists());
    assert!(!zsh_path(home.path()).exists());
    assert!(!fish_path(home.path()).exists());
}

#[test]
fn uninstall_is_silent_noop_when_nothing_installed() {
    let home = tempfile::tempdir().unwrap();
    bl()
        .args(["completions", "--uninstall"])
        .env("HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn install_without_home_errors() {
    bl()
        .args(["completions", "--install"])
        .env_remove("HOME")
        .assert()
        .failure()
        .stderr(predicate::str::contains("HOME not set"));
}

#[test]
fn no_shell_and_no_flag_errors() {
    bl()
        .arg("completions")
        .assert()
        .failure()
        .stderr(predicate::str::contains("specify a shell"));
}
