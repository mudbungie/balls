//! Tests for the read-verb `run` lifecycle: the §4 read-op narration
//! (begin/done/abort at `debug` — absent at the default `info` threshold) and
//! the §6 op-uniform read dispatch (a wired `list` hook key actually runs;
//! `--json` never dispatches).

use std::fs;

use tempfile::TempDir;

use super::test_support::{self, bind_script, landing_with, op_log, task};
use super::{parse, render, run};
use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::log::Level;
use crate::task::Task;
use crate::verb::Verb;

/// An edge rooted in `tmp` whose store is seeded with `tasks`.
fn edge_with(tmp: &TempDir, tasks: &[(&str, Task)]) -> Edge {
    let edge = test_support::edge(tmp.path(), 0);
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    for (id, t) in tasks {
        crate::taskfile::write_task(&store, id, t).unwrap();
    }
    edge
}

#[test]
fn run_dispatches_each_read_verb() {
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[("bl-1", task("One", 1))]);
    run(&edge, Verb::List, &[]).unwrap();
    run(&edge, Verb::List, &["--status".into(), "ready".into()]).unwrap();
    run(&edge, Verb::Show, &["bl-1".into()]).unwrap();
}

#[test]
fn run_errors_on_a_missing_ball_and_a_non_read_verb() {
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[]);
    assert!(run(&edge, Verb::Show, &["bl-x".into()]).is_err());
    assert!(run(&edge, Verb::Create, &[]).is_err()); // not a read verb
}

#[test]
fn run_lists_and_shows_dead_balls_from_history() {
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[]);
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    let gs = test_support::git_store_at(&store);
    gs.create("bl-dead", &task("Gone", 1), 1).retire("bl-dead", "close", 9);

    // --status closed / --all reach the dead set, and show falls through to it.
    run(&edge, Verb::List, &["--status".into(), "closed".into()]).unwrap();
    run(&edge, Verb::List, &["--all".into()]).unwrap();
    run(&edge, Verb::Show, &["bl-dead".into()]).unwrap();
}

#[test]
fn a_read_narrates_begin_and_done_at_debug_only() {
    let tmp = TempDir::new().unwrap();
    let mut edge = edge_with(&tmp, &[("bl-1", task("One", 1))]);
    // The default threshold (`info`): zero records at any point of the read —
    // §4 keeps read chatter out of the log.
    run(&edge, Verb::List, &[]).unwrap();
    assert_eq!(op_log(&edge), "");
    // `--log-level debug` (the CLI layer, §4): begin/done land, op-stamped.
    edge.log_level = Some("debug".into());
    run(&edge, Verb::List, &[]).unwrap();
    let log = op_log(&edge);
    assert!(log.contains(r#""lvl":"debug","src":"core","op":"list","msg":"begin""#));
    assert!(log.contains(r#""lvl":"debug","src":"core","op":"list","msg":"done""#));
}

#[test]
fn a_failing_read_narrates_abort_at_debug() {
    let tmp = TempDir::new().unwrap();
    let mut edge = edge_with(&tmp, &[]);
    let gs = test_support::git_store_at(&edge.xdg.clone_dir(&edge.invocation_path).store());
    gs.create("bl-other", &task("Other", 1), 1); // a non-empty history to miss in
    edge.log_level = Some("debug".into());
    assert!(run(&edge, Verb::Show, &["bl-x".into()]).is_err());
    let log = op_log(&edge);
    assert!(log.contains(r#""msg":"begin""#));
    assert!(log.contains(r#""lvl":"debug","src":"core","op":"show","msg":"abort no such ball: bl-x""#));
}

#[test]
fn run_fails_loud_on_a_bad_threshold_source() {
    // Every `log_level` source parses strictly (§4, bl-56f7) — a read included.
    let tmp = TempDir::new().unwrap();
    let mut edge = edge_with(&tmp, &[]);
    edge.log_level = Some("noisy".into());
    assert!(run(&edge, Verb::List, &[]).is_err());
    // …and a malformed landing config cannot resolve the §4 stack at all.
    edge.log_level = None;
    let landing = landing_with(&edge, "[hooks]\n");
    fs::write(landing.join("config").join("balls.toml"), "not = toml = at all").unwrap();
    assert!(run(&edge, Verb::List, &[]).is_err());
}

/// Wire `fake` on the `list` bare hook key (§6), with a script that appends
/// its argv to `tmp/hits` — `run` prints to the real stdout, so the dispatch
/// is observed by its side effect.
fn wire_listing_hooks(tmp: &TempDir, edge: &Edge) {
    let landing = landing_with(edge, "[hooks]\n\"list\" = [\"fake\"]\n");
    let script = format!(
        "#!/bin/sh\npayload=$(cat)\ncase \"$payload\" in *bl-id*) exit 1;; esac\necho \"$1 $2\" >> {}/hits\n",
        tmp.path().display()
    );
    bind_script(tmp.path(), &landing, "fake", &script);
}

#[test]
fn a_wired_list_hook_actually_dispatches() {
    // Reads are not special-cased (§6): the bare `list` key runs through the
    // same fold `show` uses. The script asserts the target-free wire (no
    // `bl-id`) and records the `list read` invocation.
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[("bl-1", task("One", 1))]);
    wire_listing_hooks(&tmp, &edge);
    run(&edge, Verb::List, &[]).unwrap();
    assert_eq!(fs::read_to_string(tmp.path().join("hits")).unwrap(), "list read\n");
}

#[test]
fn json_reads_never_dispatch() {
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[("bl-1", task("One", 1))]);
    wire_listing_hooks(&tmp, &edge);
    run(&edge, Verb::List, &["--json".into()]).unwrap();
    assert!(!tmp.path().join("hits").exists());
}

#[test]
fn the_folded_line_lands_in_the_human_render() {
    // `render` returns the output `run` prints: a wired plugin's stdout folds
    // in after the listing.
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[("bl-1", task("One", 1))]);
    let landing = landing_with(&edge, "[hooks]\n\"list\" = [\"fake\"]\n");
    bind_script(tmp.path(), &landing, "fake", "#!/bin/sh\ncat > /dev/null\necho folded-line\n");
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    let (cfg, log) = (EffectiveConfig::default(), test_support::log_at(tmp.path(), Level::Debug, Verb::List));
    let flags = parse(Verb::List, &[]).unwrap();
    let out = render(&edge, Verb::List, &flags, &store, &cfg, &log).unwrap();
    assert!(out.contains("bl-1") && out.ends_with("folded-line\n"), "{out}");
}

#[test]
fn legacy_reads_project_the_old_store_through_the_ordinary_render() {
    // §16 shim: `--legacy` swaps the catalog SOURCE only — the greenfield store
    // stays untouched and the downstream render is the normal read path.
    let tmp = TempDir::new().unwrap();
    let edge = edge_with(&tmp, &[("bl-new", task("Greenfield", 1))]);
    test_support::legacy_store_at(
        &tmp.path().join("proj"),
        &[(
            "bl-old.json",
            r#"{"id":"bl-old","title":"Old one","status":"open",
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        )],
    );
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    let (cfg, log) = (EffectiveConfig::default(), test_support::log_at(tmp.path(), Level::Info, Verb::List));
    // list --legacy is the migration preview: the legacy ball, not the store's.
    let flags = parse(Verb::List, &["--legacy".into(), "--plain".into()]).unwrap();
    let out = render(&edge, Verb::List, &flags, &store, &cfg, &log).unwrap();
    assert!(out.contains("bl-old") && !out.contains("bl-new"), "{out}");
    // show --legacy resolves one projected ball; a miss NEVER falls through to
    // greenfield history — the legacy set is the whole world the flag names.
    let flags = parse(Verb::Show, &["bl-old".into(), "--legacy".into(), "--plain".into()]).unwrap();
    let out = render(&edge, Verb::Show, &flags, &store, &cfg, &log).unwrap();
    assert!(out.contains("Old one"), "{out}");
    let flags = parse(Verb::Show, &["bl-new".into(), "--legacy".into()]).unwrap();
    let err = render(&edge, Verb::Show, &flags, &store, &cfg, &log).unwrap_err();
    assert!(err.to_string().contains("no such legacy ball: bl-new"), "{err}");
}
