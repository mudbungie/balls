//! Tests for the §8 op-instant ladder ([`super`]) — the pure `resolve` ladder
//! exhaustively (every rung and every fail-open fall-through), the `locate`
//! resolution of a `clock_provider` value (absolute path vs PATH-resolved name,
//! bl-cfe3), `probe`'s protocol reads, the `git_date_env` format, and the
//! `for_op` edge wrapper on a throwaway checkout.

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

/// An executable shell script at `dir/name` whose body is `body` — a fake clock
/// provider. Returns its path.
fn bin(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// A `locate` closure over `dir`: a name resolves to `dir/name` when that is a
/// file, mirroring [`super::locate`]'s hit path so the pure ladder is exercised
/// without the real edge filesystem lookup.
fn in_dir(dir: &Path) -> impl Fn(&str) -> Option<PathBuf> + '_ {
    move |name: &str| {
        let p = dir.join(name);
        p.is_file().then_some(p)
    }
}

#[test]
fn no_provider_falls_to_balls_clock_then_the_system_clock() {
    // BALLS_CLOCK set → that instant, cleanly (no note).
    let i = resolve(None, |_| None, Some(42), || 999);
    assert_eq!(i.t, 42);
    assert!(i.note.is_none());
    // Nothing set → the injected system clock, cleanly.
    let i = resolve(None, |_| None, None, || 999);
    assert_eq!(i.t, 999);
    assert!(i.note.is_none());
}

#[test]
fn a_resolved_provider_printing_an_integer_outranks_every_lower_rung() {
    let d = TempDir::new().unwrap();
    bin(d.path(), "clk", "echo 1700000000");
    let i = resolve(Some("clk"), in_dir(d.path()), Some(42), || 999);
    assert_eq!(i.t, 1_700_000_000); // beats BALLS_CLOCK and the system clock
    assert!(i.note.is_none());
}

#[test]
fn an_unresolvable_provider_falls_open_with_a_note() {
    let i = resolve(Some("ghost"), |_| None, Some(42), || 999);
    assert_eq!(i.t, 42); // fell to BALLS_CLOCK
    assert!(i.note.unwrap().contains("clock_provider ghost not found"));
}

#[test]
fn a_provider_exiting_nonzero_falls_open_with_a_note() {
    let d = TempDir::new().unwrap();
    bin(d.path(), "clk", "exit 3");
    let i = resolve(Some("clk"), in_dir(d.path()), None, || 999);
    assert_eq!(i.t, 999); // fell to the system clock
    assert!(i.note.unwrap().contains("clk:"));
}

#[test]
fn a_provider_printing_a_non_integer_falls_open() {
    let d = TempDir::new().unwrap();
    bin(d.path(), "clk", "echo not-a-number");
    let i = resolve(Some("clk"), in_dir(d.path()), Some(7), || 999);
    assert_eq!(i.t, 7);
    assert!(i.note.unwrap().contains("non-integer"));
}

#[test]
fn a_provider_printing_nothing_falls_open() {
    let d = TempDir::new().unwrap();
    bin(d.path(), "clk", "true"); // exit 0, empty stdout
    let i = resolve(Some("clk"), in_dir(d.path()), Some(5), || 0);
    assert_eq!(i.t, 5);
    assert!(i.note.is_some());
}

#[test]
fn probe_reads_the_first_trimmed_line_only() {
    let d = TempDir::new().unwrap();
    // Leading/trailing whitespace trimmed; lines past the first ignored.
    bin(d.path(), "clk", "printf '  123 \\nextra\\n'");
    let i = resolve(Some("clk"), in_dir(d.path()), None, || 0);
    assert_eq!(i.t, 123);
    assert!(i.note.is_none());
}

/// A minimal edge with the given `exe_dir` (the beside-`bl` rung) and `$PATH`
/// dirs — the two inputs [`super::locate`] reads for a bare name.
fn edge_with(exe_dir: Option<PathBuf>, path_dirs: Vec<PathBuf>) -> Edge {
    let home = PathBuf::from("/home");
    Edge {
        xdg: crate::layout::Xdg::with(&home, None, Some("/state")),
        invocation_path: PathBuf::from("/proj"),
        default_actor: "t".into(),
        depth: 0,
        exe_dir,
        path_dirs,
        color: false,
        log_level: None,
        balls_clock: None,
    }
}

#[test]
fn locate_uses_an_absolute_path_verbatim_when_it_is_a_file() {
    let d = TempDir::new().unwrap();
    let abs = bin(d.path(), "clk", "echo 1"); // an absolute path to a real file
    let e = edge_with(None, Vec::new());
    assert_eq!(locate(&abs.to_string_lossy(), &e), Some(abs.clone()));
    // A non-existent absolute path resolves to nothing — the ladder falls open.
    assert_eq!(locate(&d.path().join("absent").to_string_lossy(), &e), None);
}

#[test]
fn locate_resolves_a_bare_name_beside_bl_then_on_path() {
    let d = TempDir::new().unwrap();
    let beside = d.path().join("beside");
    let onpath = d.path().join("onpath");
    fs::create_dir_all(&beside).unwrap();
    fs::create_dir_all(&onpath).unwrap();
    bin(&onpath, "clk", "echo 1");
    // On PATH only → found there.
    let e = edge_with(Some(beside.clone()), vec![onpath.clone()]);
    assert_eq!(locate("clk", &e), Some(onpath.join("clk")));
    // Beside bl wins over PATH (the seed sibling rule).
    let side = bin(&beside, "clk", "echo 2");
    assert_eq!(locate("clk", &e), Some(side));
    // A name nowhere → None.
    assert_eq!(locate("ghost", &e), None);
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
    // A fresh edge with no configured provider → BALLS_CLOCK wins, exercising the
    // impure wrapper end to end (the local-layer config read + the ladder).
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
