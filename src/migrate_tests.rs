//! Unit-level tests for `migrate` — the integration / end-to-end
//! gates live in `tests/conformance_migrate*.rs`. This file exercises
//! the pure helpers reachable without a real git repo.

use super::*;

#[test]
fn report_lines_round_trip_strings() {
    let r = Report(vec!["a".into(), "b".into()]);
    assert_eq!(r.lines().collect::<Vec<&str>>(), vec!["a", "b"]);
}

#[test]
fn detection_reports_not_balls_outside_a_balls_clone() {
    let dir = tempfile::TempDir::new().unwrap();
    let res = detect(dir.path()).expect("detect on a plain dir");
    assert!(matches!(res, Detection::NotBalls));
}

#[test]
fn detection_finds_legacy_when_config_json_exists() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    std::fs::write(dir.path().join(".balls/config.json"), "{}").unwrap();
    let res = detect(dir.path()).expect("detect");
    assert!(matches!(res, Detection::Legacy));
}

#[test]
fn run_reports_no_balls_state_in_a_plain_git_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["init", "-q", "-b", "main"])
        .status()
        .unwrap();
    let err = run(dir.path()).expect_err("plain git repo has no balls state");
    let msg = format!("{err}");
    assert!(
        msg.contains("no balls state to migrate"),
        "expected NotBalls diagnostic; got {msg}"
    );
}

/// `detect` on a git repo whose `origin` resolves to an XDG tracker
/// checkout that does NOT exist returns `NotBalls`. Covers the
/// "origin set, no XDG checkout materialized" branch — the second
/// `NotBalls` return path in `detect`.
#[test]
fn detection_reports_not_balls_with_origin_but_no_xdg_checkout() {
    let dir = tempfile::TempDir::new().unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["init", "-q", "-b", "main"])
        .status()
        .unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["remote", "add", "origin", "git@host:org/nowhere.git"])
        .status()
        .unwrap();
    let res = detect(dir.path()).expect("detect");
    assert!(matches!(res, Detection::NotBalls));
}

/// A legacy clone with `.balls/config.json` but no `origin` remote
/// (i.e. a legacy stealth-style setup) aborts migration with the
/// "no origin" diagnostic. Covers the `origin_url` `ok_or_else`
/// branch inside `execute`.
#[test]
fn run_aborts_legacy_clone_with_no_origin() {
    let dir = tempfile::TempDir::new().unwrap();
    crate::git::clean_git_command(dir.path())
        .args(["init", "-q", "-b", "main"])
        .status()
        .unwrap();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    std::fs::write(
        dir.path().join(".balls/config.json"),
        serde_json::to_string(&crate::config::Config::default()).unwrap(),
    )
    .unwrap();
    let err = run(dir.path()).expect_err("no origin → migrate aborts");
    let msg = format!("{err}");
    assert!(
        msg.contains("no `origin` remote"),
        "expected no-origin diagnostic; got {msg}"
    );
}
