//! Shared fixtures for the `native_participant` test files. Two test
//! modules consume these (`*_describe_tests.rs` and
//! `*_proto_tests.rs`); keeping them out of either avoids a 300+
//! line monolith and a duplicated copy.

use crate::plugin::native_types::{DescribeResponse, ProjectionWire};
use crate::config::PluginEntry;
use crate::participant::Event;
use crate::store::Store;
use crate::task::{NewTaskOpts, Task, TaskType};

pub(crate) fn stealth_store() -> (tempfile::TempDir, Store) {
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

pub(crate) fn entry() -> PluginEntry {
    PluginEntry {
        enabled: true,
        sync_on_change: true,
        config_file: ".balls/plugins/x.json".into(),
        participant: None,
    }
}

pub(crate) fn save_task(store: &Store, id: &str) -> Task {
    let opts = NewTaskOpts {
        title: "test".into(),
        task_type: TaskType::task(),
        priority: 3,
        parent: None,
        depends_on: vec![],
        description: String::new(),
        tags: vec![],
    };
    let task = Task::new(opts, id.into());
    store.save_task(&task).unwrap();
    task
}

pub(crate) fn describe_for(events: &[Event]) -> DescribeResponse {
    DescribeResponse {
        subscriptions: events.to_vec(),
        projection: ProjectionWire {
            external_prefixes: vec!["jira".into()],
            ..ProjectionWire::default()
        },
        retry_budget: None,
    }
}
