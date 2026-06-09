//! Tests for `bl conf` reads (bl-c2de) — the §4 key namespace, per-layer
//! resolution with provenance, the §12 remote ladder's read, and the
//! dump/single-key dispatch.

use super::*;
use crate::layout::Xdg;
use crate::substrate;
use std::fs;
use tempfile::TempDir;

fn edge(tmp: &TempDir) -> Edge {
    Edge {
        xdg: Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir: None,
        color: false,
        log_level: None,
    }
}

/// Found the substrate (a seeded landing — `balls.toml` carries the standard
/// values, the plugin-less box prunes every `[hooks]` entry) for `e`.
fn founded(e: &Edge) -> CloneDir {
    let clone = e.xdg.clone_dir(&e.invocation_path);
    substrate::found(&clone.landing(), &clone.store(), &e.xdg, None).unwrap();
    clone
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

fn user_file(e: &Edge, name: &str, body: &str) {
    let path = e.xdg.user_config().with_file_name(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn landing_file(clone: &CloneDir, name: &str, body: &str) {
    fs::write(clone.landing().join("config").join(name), body).unwrap();
}

/// Resolve one key to its `(value, layer)` pair.
fn res(e: &Edge, clone: &CloneDir, key: &str) -> (String, String) {
    let r = resolve(e, clone, &Key::parse(key).unwrap()).unwrap();
    (r.value, r.layer)
}

#[test]
fn the_key_namespace_accepts_each_home_and_rejects_the_rest() {
    for ok in ["task-remote", "task-branch", "log-level", "claim.pre", "close.post", "show", "list"] {
        assert!(Key::parse(ok).is_ok(), "{ok} should parse");
    }
    // No such key; `conf` is chainless; reads dispatch BARE keys (§6), so a
    // phased read key and a bare phased-op key are both refused.
    for bad in ["frob", "conf.pre", "show.pre", "list.post", "claim.mid", "claim"] {
        let err = Key::parse(bad).unwrap_err().to_string();
        assert!(err.contains("unknown key"), "{bad}: {err}");
    }
}

#[test]
fn conf_requires_a_founded_checkout() {
    let tmp = TempDir::new().unwrap();
    let err = run(&edge(&tmp), &[]).unwrap_err().to_string();
    assert!(err.contains("run `bl prime` first"), "{err}");
}

#[test]
fn a_scalar_resolves_innermost_wins_and_names_the_layer() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    // The seed wrote the standard values onto the landing.
    assert_eq!(res(&e, &clone, "task-branch"), ("balls/tasks".into(), "landing".into()));
    // The per-machine XDG layer outranks the landing (§4 layer 2 over 3).
    user_file(&e, "config.toml", "tasks_branch = \"balls/x\"\n");
    assert_eq!(res(&e, &clone, "task-branch"), ("balls/x".into(), "xdg".into()));
    // No layer sets it → the serde default answers.
    user_file(&e, "config.toml", "");
    landing_file(&clone, "balls.toml", "");
    assert_eq!(res(&e, &clone, "task-branch"), ("balls/tasks".into(), "default".into()));
}

#[test]
fn the_log_level_cli_tier_outranks_every_file_layer() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    assert_eq!(res(&e, &clone, "log-level"), ("info".into(), "landing".into()));
    let cli = Edge { log_level: Some("debug".into()), ..e };
    assert_eq!(res(&cli, &clone, "log-level"), ("debug".into(), "cli".into()));
}

#[test]
fn task_remote_reads_the_durable_ladder_and_shows_stealth() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    // No XDG remote, no project origin → visible stealth (bl-d234).
    assert_eq!(res(&e, &clone, "task-remote"), ("(none)".into(), "stealth".into()));
    // A project repo with an `origin` is the bottom durable tier (§12).
    fs::create_dir_all(&e.invocation_path).unwrap();
    crate::git::run(&e.invocation_path, &["init", "-q"], None).unwrap();
    crate::git::run(&e.invocation_path, &["remote", "add", "origin", "git@hub:o"], None).unwrap();
    assert_eq!(res(&e, &clone, "task-remote"), ("git@hub:o".into(), "origin".into()));
    // The XDG `task-remote` outranks origin.
    user_file(&e, "config.toml", "remote = \"git@hub:x\"\n");
    assert_eq!(res(&e, &clone, "task-remote"), ("git@hub:x".into(), "xdg".into()));
}

#[test]
fn a_hooks_key_resolves_the_composed_list_and_names_its_layers() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    landing_file(&clone, "plugins.toml", "[hooks]\n\"close.pre\" = [\"x\", \"y\"]\n");
    assert_eq!(res(&e, &clone, "close.pre"), ("x, y".into(), "landing".into()));
    // An XDG directive composes with the landing list — both layers answer.
    user_file(&e, "plugins.toml", "[hooks]\n\"close.pre_append\" = [\"z\"]\n\"claim.post\" = [\"w\"]\n");
    assert_eq!(res(&e, &clone, "close.pre"), ("x, y, z".into(), "landing+xdg".into()));
    assert_eq!(res(&e, &clone, "claim.post"), ("w".into(), "xdg".into()));
    // An unwired key reads empty, from no layer.
    assert_eq!(res(&e, &clone, "update.pre"), ("(none)".into(), "default".into()));
}

#[test]
fn a_non_table_hooks_entry_mentions_nothing() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    landing_file(&clone, "plugins.toml", "hooks = \"nope\"\n");
    assert_eq!(res(&e, &clone, "close.pre"), ("(none)".into(), "default".into()));
}

#[test]
fn the_dump_renders_scalars_hook_rows_and_paths() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    // One filled list and one emptied one — the latter renders `(none)`.
    landing_file(&clone, "plugins.toml", "[hooks]\n\"close.pre\" = [\"x\"]\n\"claim.post\" = []\n");
    run(&e, &[]).unwrap();
}

#[test]
fn a_single_key_read_resolves_and_rejects_stray_values() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    founded(&e);
    run(&e, &args(&["task-branch"])).unwrap();
    let err = run(&e, &args(&["bogus"])).unwrap_err().to_string();
    assert!(err.contains("unknown key"), "{err}");
    let err = run(&e, &args(&["task-branch", "extra"])).unwrap_err().to_string();
    assert!(err.contains("takes no value on a read"), "{err}");
}

#[test]
fn the_conf_verb_dispatches_through_the_front_door() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    assert_eq!(crate::run(&e, &args(&["conf"])), 1); // not founded → op error
    founded(&e);
    assert_eq!(crate::run(&e, &args(&["conf"])), 0);
}
