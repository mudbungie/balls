//! Tests for the §8 op-instant ladder ([`super`]) — the pure `resolve` ladder
//! exhaustively (every rung and every fail-open fall-through), `probe`'s protocol
//! reads, the `git_date_env` format, and the `for_op` edge wrapper on a throwaway
//! landing.

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

/// A registry over `dir` (a landing) with `name` bound to an executable shell
/// script whose body is `body` — the fake clock provider.
fn provider(dir: &Path, name: &str, body: &str) -> Registry {
    let bin = dir.join(name);
    fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    let reg = Registry::at(dir);
    reg.bind(name, &bin).unwrap();
    reg
}

#[test]
fn no_provider_falls_to_balls_clock_then_the_system_clock() {
    let d = TempDir::new().unwrap();
    let reg = Registry::at(d.path());
    // BALLS_CLOCK set → that instant, cleanly (no note).
    let i = resolve(None, &reg, Some(42), || 999);
    assert_eq!(i.t, 42);
    assert!(i.note.is_none());
    // Nothing set → the injected system clock, cleanly.
    let i = resolve(None, &reg, None, || 999);
    assert_eq!(i.t, 999);
    assert!(i.note.is_none());
}

#[test]
fn a_bound_provider_printing_an_integer_outranks_every_lower_rung() {
    let d = TempDir::new().unwrap();
    let reg = provider(d.path(), "clk", "echo 1700000000");
    let i = resolve(Some("clk"), &reg, Some(42), || 999);
    assert_eq!(i.t, 1_700_000_000); // beats BALLS_CLOCK and the system clock
    assert!(i.note.is_none());
}

#[test]
fn an_unbound_provider_falls_open_with_a_note() {
    let d = TempDir::new().unwrap();
    let reg = Registry::at(d.path()); // nothing bound
    let i = resolve(Some("ghost"), &reg, Some(42), || 999);
    assert_eq!(i.t, 42); // fell to BALLS_CLOCK
    assert!(i.note.unwrap().contains("clock_provider ghost not bound"));
}

#[test]
fn a_provider_exiting_nonzero_falls_open_with_a_note() {
    let d = TempDir::new().unwrap();
    let reg = provider(d.path(), "clk", "exit 3");
    let i = resolve(Some("clk"), &reg, None, || 999);
    assert_eq!(i.t, 999); // fell to the system clock
    assert!(i.note.unwrap().contains("clk:"));
}

#[test]
fn a_provider_printing_a_non_integer_falls_open() {
    let d = TempDir::new().unwrap();
    let reg = provider(d.path(), "clk", "echo not-a-number");
    let i = resolve(Some("clk"), &reg, Some(7), || 999);
    assert_eq!(i.t, 7);
    assert!(i.note.unwrap().contains("non-integer"));
}

#[test]
fn a_provider_printing_nothing_falls_open() {
    let d = TempDir::new().unwrap();
    let reg = provider(d.path(), "clk", "true"); // exit 0, empty stdout
    let i = resolve(Some("clk"), &reg, Some(5), || 0);
    assert_eq!(i.t, 5);
    assert!(i.note.is_some());
}

#[test]
fn probe_reads_the_first_trimmed_line_only() {
    let d = TempDir::new().unwrap();
    // Leading/trailing whitespace trimmed; lines past the first ignored.
    let reg = provider(d.path(), "clk", "printf '  123 \\nextra\\n'");
    let i = resolve(Some("clk"), &reg, None, || 0);
    assert_eq!(i.t, 123);
    assert!(i.note.is_none());
}

#[test]
fn git_date_env_pins_both_dates_to_the_instant() {
    assert_eq!(
        git_date_env(1_700_000_000),
        [("GIT_AUTHOR_DATE", "@1700000000".to_string()), ("GIT_COMMITTER_DATE", "@1700000000".to_string())]
    );
}

#[test]
fn for_op_reads_config_then_resolves_the_ladder() {
    // A fresh edge with no landing config → provider absent → BALLS_CLOCK wins,
    // exercising the impure wrapper end to end (config read + registry + ladder).
    let tmp = TempDir::new().unwrap();
    let edge = crate::edge::Edge {
        xdg: crate::layout::Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "t".into(),
        depth: 0,
        exe_dir: None,
        path_dirs: Vec::new(),
        color: false,
        log_level: None,
        balls_clock: Some(1_555_000_000),
    };
    let i = for_op(&edge).unwrap();
    assert_eq!(i.t, 1_555_000_000);
    assert!(i.note.is_none());
}
