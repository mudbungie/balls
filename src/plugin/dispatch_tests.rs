//! Unit coverage for the legacy-plugin dispatcher. The integration
//! tests under `tests/plugin_*.rs` already verify byte-identical
//! behavior end-to-end; these tests pin the dispatcher's contract so
//! later refactors can't bypass it without touching this file.

use super::*;
use crate::project_config::{PluginEntry, ProjectConfig};
use crate::error::Result;
use crate::participant::Event;
use crate::participant_config::InvocationOverrides;
use crate::plugin::runner::test_seam::ExecutableOverride;
use crate::store::Store;
use crate::task::{NewTaskOpts, Task, TaskType};
use std::collections::BTreeMap;

/// Minimal push: no pre-image, no overrides — the legacy-parity shape
/// these dispatcher contract tests exercise.
fn push(store: &Store, task: &Task, event: Event) -> Result<DispatchOutcome> {
    let ov = InvocationOverrides::default();
    dispatch_push(&DispatchInput {
        store,
        task_before: None,
        task,
        event,
        identity: "alice",
        commit: None,
        overrides: &ov,
        override_tokens: &[],
    })
}

fn stealth_store() -> (tempfile::TempDir, Store) {
    let td = tempfile::tempdir().unwrap();
    let tasks_dir = td.path().join("tasks");
    let store = Store::init(
        td.path(),
        true,
        Some(tasks_dir.to_string_lossy().into_owned()),
    )
    .unwrap();
    (td, store)
}

fn write_config(store: &Store, plugins: BTreeMap<String, PluginEntry>) {
    let cfg = ProjectConfig { plugins, ..ProjectConfig::default() };
    cfg.save(&store.project_config_path()).unwrap();
}

fn make_task(store: &Store, title: &str) -> Task {
    let opts = NewTaskOpts {
        title: title.into(),
        task_type: TaskType::task(),
        priority: 3,
        parent: None,
        depends_on: vec![],
        description: String::new(),
        tags: vec![],
    };
    let task = Task::new(opts, "bl-1234".into());
    store.save_task(&task).unwrap();
    task
}

fn entry(enabled: bool, sync_on_change: bool) -> PluginEntry {
    PluginEntry {
        enabled,
        sync_on_change,
        config_file: ".balls/plugins/x.json".into(),
        participant: None,
    }
}

#[test]
fn dispatch_push_with_no_plugins_is_ok() {
    let (_td, store) = stealth_store();
    write_config(&store, BTreeMap::new());
    let task = make_task(&store, "no plugins");
    push(&store, &task, Event::Claim).unwrap();
}

#[test]
fn dispatch_push_skips_disabled_plugin() {
    // A disabled plugin must not be dispatched. The participant
    // would otherwise try to spawn a non-existent executable.
    let (_td, store) = stealth_store();
    let mut plugins = BTreeMap::new();
    plugins.insert("nope".into(), entry(false, true));
    write_config(&store, plugins);
    let task = make_task(&store, "disabled plugin");
    push(&store, &task, Event::Claim).unwrap();
}

#[test]
fn dispatch_push_skips_when_event_not_subscribed() {
    // sync_on_change=false: only the Sync event is subscribed. A
    // dispatch_push for Claim must skip without spawning anything.
    let (_td, store) = stealth_store();
    let mut plugins = BTreeMap::new();
    plugins.insert("sync-only".into(), entry(true, false));
    write_config(&store, plugins);
    let task = make_task(&store, "sync only");
    push(&store, &task, Event::Claim).unwrap();
}

#[test]
fn dispatch_push_propagates_config_load_error() {
    let (_td, store) = stealth_store();
    std::fs::write(store.project_config_path(), "not json").unwrap();
    let task = Task::new(
        NewTaskOpts {
            title: "x".into(),
            task_type: TaskType::task(),
            priority: 3,
            parent: None,
            depends_on: vec![],
            description: String::new(),
            tags: vec![],
        },
        "bl-1234".into(),
    );
    let err = push(&store, &task, Event::Update).unwrap_err();
    let _ = format!("{err}");
}

#[test]
fn dispatch_sync_with_no_plugins_returns_empty() {
    let (_td, store) = stealth_store();
    write_config(&store, BTreeMap::new());
    let reports = dispatch_sync(&store, None, "alice").unwrap();
    assert!(reports.is_empty());
}

#[test]
fn dispatch_sync_skips_disabled_plugin() {
    let (_td, store) = stealth_store();
    let mut plugins = BTreeMap::new();
    plugins.insert("disabled".into(), entry(false, true));
    write_config(&store, plugins);
    let reports = dispatch_sync(&store, None, "alice").unwrap();
    assert!(reports.is_empty());
}

#[test]
fn dispatch_sync_runs_for_unavailable_executable() {
    // A configured-but-unresolvable plugin: `Plugin::auth_check`
    // returns false, propose returns Other, BestEffort absorbs as
    // Skipped, and the dispatcher records no report. Critical
    // because legacy behavior treats "plugin missing" as silent skip.
    let (_td, store) = stealth_store();
    let mut plugins = BTreeMap::new();
    plugins.insert("ghost".into(), entry(true, true));
    write_config(&store, plugins);
    let _exe = ExecutableOverride::unresolvable(&store.root);
    let reports = dispatch_sync(&store, None, "alice").unwrap();
    assert!(reports.is_empty());
}

#[test]
fn dispatch_sync_propagates_config_load_error() {
    let (_td, store) = stealth_store();
    std::fs::write(store.project_config_path(), "not json").unwrap();
    let _ = dispatch_sync(&store, None, "alice").unwrap_err();
}

#[test]
fn dispatch_sync_filter_threads_through() {
    // Verifies the filter argument is plumbed into the participant
    // (without spawning a real plugin). Configured-but-missing plugin
    // → Skipped; the call still returns Ok with no reports.
    let (_td, store) = stealth_store();
    let mut plugins = BTreeMap::new();
    plugins.insert("filterable".into(), entry(true, true));
    write_config(&store, plugins);
    let _exe = ExecutableOverride::unresolvable(&store.root);
    let reports = dispatch_sync(&store, Some("PROJ-1"), "alice").unwrap();
    assert!(reports.is_empty());
}
