//! The READ half of `bl conf` (bl-d2d8): the full provenance dump, the
//! single-key read's value+provenance, the §12 `task-remote` ladder walked
//! across every durable layer, and the pre-prime refusal.

use predicates::str::contains;
use tempfile::TempDir;

use crate::{bare_project, bl, git, primed_project, read_is};

/// `bl conf` (no args) dumps every scalar's value + answering layer, then the
/// three path lines. A fresh checkout with no `origin` reads circumstantial
/// stealth, and the seeded landing answers `task-branch`/`log-level`.
#[test]
fn dump_lists_scalar_rows_and_path_lines() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    let out = bl(&p, &h, &s).arg("conf").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();

    // Every scalar row: "<key> ...pad... <value> ...pad... <layer>".
    for (key, value, layer) in [
        ("task-remote", "(none)", "stealth"),
        ("task-branch", "balls/tasks", "landing"),
        ("log-level", "info", "landing"),
        ("clock-provider", "(none)", "default"),
    ] {
        let row = text.lines().find(|l| l.starts_with(key)).unwrap_or_else(|| panic!("no {key} row in {text}"));
        assert!(row.contains(value), "row {key}: value {value:?} missing: {row:?}");
        assert!(row.trim_end().ends_with(layer), "row {key}: layer {layer:?} missing: {row:?}");
    }
    // The path block: xdg / landing / store, each on its own labelled line.
    for label in ["xdg ", "landing ", "store "] {
        assert!(text.lines().any(|l| l.starts_with(label)), "dump missing {label:?} path line: {text}");
    }
}

/// `task-remote` resolves through the §12 durable ladder, innermost wins, and
/// the read NAMES the tier that answered — the whole provenance point.
#[test]
fn task_remote_provenance_walks_every_layer() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    // 1. Fresh, no origin ⇒ circumstantial stealth (nothing set anywhere).
    read_is(&p, &h, &s, "task-remote", "(none)", "stealth");

    // 2. A project `origin` answers by name (a local get-url, never contacted).
    git(&p, &["remote", "add", "origin", "git@example.com:proj.git"]);
    read_is(&p, &h, &s, "task-remote", "git@example.com:proj.git", "origin");

    // 3. The legacy global XDG remote outranks origin, and is labelled global.
    std::fs::create_dir_all(h.join(".config").join("balls")).unwrap();
    std::fs::write(h.join(".config").join("balls").join("config.toml"), "remote = \"git@xdg:g.git\"\n").unwrap();
    read_is(&p, &h, &s, "task-remote", "git@xdg:g.git", "xdg (global)");

    // 4. This clone's binding remote outranks the global one.
    bl(&p, &h, &s).args(["conf", "set", "task-remote", "git@host:r.git"]).assert().success();
    read_is(&p, &h, &s, "task-remote", "git@host:r.git", "binding");

    // 5. A declared-stealth sentinel on the landing outranks everything below —
    //    the binding URL is still on disk, yet the landing policy wins.
    bl(&p, &h, &s).args(["conf", "set", "task-remote", "none"]).assert().success();
    read_is(&p, &h, &s, "task-remote", "(none)", "landing");
}

/// The scalar reads name their answering layer too: the seeded landing for the
/// balls.toml fields, `default` for an unset clock provider.
#[test]
fn scalar_reads_name_their_layer() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());
    read_is(&p, &h, &s, "task-branch", "balls/tasks", "landing");
    read_is(&p, &h, &s, "log-level", "info", "landing");
    read_is(&p, &h, &s, "clock-provider", "(none)", "default");
}

/// A legacy global XDG `clock_provider` answers `clock-provider` when this clone
/// has no binding value — the fail-open box-local tier (§8, bl-cfe3).
#[test]
fn clock_provider_reads_the_global_xdg_layer() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());
    std::fs::create_dir_all(h.join(".config").join("balls")).unwrap();
    std::fs::write(h.join(".config").join("balls").join("config.toml"), "clock_provider = \"/bin/echo\"\n").unwrap();
    read_is(&p, &h, &s, "clock-provider", "/bin/echo", "xdg (global)");
}

/// `conf` before `prime` has no checkout to read and refuses, nonzero, naming
/// the fix — it never fabricates a store.
#[test]
fn conf_before_prime_refuses() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = bare_project(tmp.path());
    bl(&p, &h, &s).arg("conf").assert().failure().stderr(contains("run `bl prime` first"));
}
