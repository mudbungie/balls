//! Dispatch of an UNBOUND schedule entry (§6/§15). A missing THIRD-PARTY plugin
//! ABORTS the op ("run bl install"); a renamed-away FIRST-PARTY plugin (the
//! [`renamed_to`] map, bl-27bf) is SKIPPED with a notice so an old committed
//! schedule degrades gracefully instead of bricking every verb the plugin rode.
//! No subprocess spawns — the binary is absent — so these need only a Log +
//! dispatcher, not the script harness in `plugin_tests`.

use super::*;
use crate::wire::{Binding, Command};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn clk() -> i64 {
    0
}

fn pref(name: &str, bin: Option<PathBuf>) -> PluginRef {
    PluginRef { name: name.into(), bin }
}

fn ctx() -> OpContext {
    OpContext {
        actor: "me".into(),
        binding: Binding {
            remote: None,
            stealth: false,
            tasks_branch: "balls/tasks".into(),
            store: "/store".into(),
            landing: "/landing".into(),
            invocation_path: "/proj".into(),
        },
        command: Some(Command { op: "close".into(), body_change: None }),
        before: None,
    }
}

#[test]
fn a_missing_third_party_plugin_aborts_and_names_bl_install() {
    let tmp = TempDir::new().unwrap();
    let log = Log::new(tmp.path().join("log"), Level::Debug, Verb::Close, clk);
    let err = Subprocess::new(ctx(), &log, 0)
        .run(&pref("ghost", None), Verb::Close, Phase::Pre, tmp.path(), None)
        .unwrap_err();
    assert!(err.to_string().contains("ghost referenced but bin/ghost missing — run bl install"));
}

#[test]
fn a_missing_renamed_first_party_plugin_is_skipped_with_a_notice() {
    // An old committed schedule naming a RENAMED first-party plugin whose binary
    // is gone is SKIPPED (the op proceeds, non-fatal) with a notice pointing at
    // the new name — not the generic abort the `ghost` case gets. This is what
    // keeps tracker→bl-tracker a small, self-diagnosing break rather than a brick.
    let tmp = TempDir::new().unwrap();
    let log_path = tmp.path().join("log");
    let log = Log::new(log_path.clone(), Level::Debug, Verb::Sync, clk);
    Subprocess::new(ctx(), &log, 0)
        .run(&pref("tracker", None), Verb::Sync, Phase::Pre, tmp.path(), None)
        .unwrap(); // Ok — skipped, not an error
    let rec = fs::read_to_string(&log_path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|r| r["src"] == "core" && r["msg"].as_str().is_some_and(|m| m.contains("was renamed")))
        .expect("a rename notice was logged");
    assert_eq!(rec["lvl"], "info");
    let msg = rec["msg"].as_str().unwrap();
    assert!(msg.contains("tracker was renamed bl-tracker"), "{msg}");
    assert!(msg.contains("bl conf"), "points at the fix: {msg}");
}
