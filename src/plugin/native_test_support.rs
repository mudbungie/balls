//! Generic store/plugin fixtures shared by the native describe tests
//! (`native_participant_describe_tests.rs`) and the protocol tests
//! (`native_participant_proto_tests.rs`). Declared at the `plugin`
//! module level so both descendant test modules can reach it; none of
//! these need `NativeProtocol`'s private state (that's in
//! `native_proto::test_helpers`).

use crate::config::PluginEntry;
use crate::participant::Event;
use crate::plugin::native_types::{DescribeResponse, ProjectionWire};
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
        wants_context: false,
    }
}
