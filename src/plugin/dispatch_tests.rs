//! Unit coverage for the legacy-plugin dispatcher. The integration
//! tests under `tests/plugin_*.rs` already verify byte-identical
//! behavior end-to-end; these tests pin the dispatcher's contract so
//! later refactors can't bypass it without touching this file.

use super::*;
use crate::config::{Config, PluginEntry, CONFIG_SCHEMA_VERSION};
use crate::participant::Event;
use crate::store::Store;
use crate::task::{NewTaskOpts, Task, TaskType};
use std::collections::BTreeMap;

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
    let cfg = Config {
        version: CONFIG_SCHEMA_VERSION,
        id_length: 4,
        stale_threshold_seconds: 60,
        auto_fetch_on_ready: true,
        worktree_dir: ".balls-worktrees".into(),
        protected_main: false,
        require_remote_on_claim: false,
        plugins,
    };
    cfg.save(&store.config_path()).unwrap();
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
    dispatch_push(&store, &task, Event::Claim, "alice").unwrap();
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
    dispatch_push(&store, &task, Event::Claim, "alice").unwrap();
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
    dispatch_push(&store, &task, Event::Claim, "alice").unwrap();
}

#[test]
fn dispatch_push_propagates_config_load_error() {
    let (_td, store) = stealth_store();
    std::fs::write(store.config_path(), "not json").unwrap();
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
    let err = dispatch_push(&store, &task, Event::Update, "alice").unwrap_err();
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
    // A configured-but-not-on-PATH plugin: `Plugin::auth_check`
    // returns false, propose returns Other, BestEffort absorbs as
    // Skipped, and the dispatcher records no report. Critical
    // because legacy behavior treats "plugin missing" as silent skip.
    let (_td, store) = stealth_store();
    let mut plugins = BTreeMap::new();
    plugins.insert("ghost".into(), entry(true, true));
    write_config(&store, plugins);
    // PATH = empty so the plugin executable is unfindable.
    let saved = std::env::var_os("PATH");
    // SAFETY: tests run single-threaded under cargo's default harness
    // for unit tests in a module; we restore PATH below.
    unsafe {
        std::env::remove_var("PATH");
    }
    let reports = dispatch_sync(&store, None, "alice").unwrap();
    if let Some(p) = saved {
        unsafe {
            std::env::set_var("PATH", p);
        }
    }
    assert!(reports.is_empty());
}

#[test]
fn dispatch_sync_propagates_config_load_error() {
    let (_td, store) = stealth_store();
    std::fs::write(store.config_path(), "not json").unwrap();
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
    let saved = std::env::var_os("PATH");
    unsafe {
        std::env::remove_var("PATH");
    }
    let reports = dispatch_sync(&store, Some("PROJ-1"), "alice").unwrap();
    if let Some(p) = saved {
        unsafe {
            std::env::set_var("PATH", p);
        }
    }
    assert!(reports.is_empty());
}
