//! Shared fixtures for the `native_participant` test files. Two test
//! modules consume these (`*_describe_tests.rs` and
//! `*_proto_tests.rs`); keeping them out of either avoids a 300+
//! line monolith and a duplicated copy.

use super::{classify, NativeProtocol};
use crate::config::PluginEntry;
use crate::negotiation::AttemptClass;
use crate::participant::Event;
use crate::plugin::native_types::{
    DescribeResponse, ProjectionWire, ProposeConflict, ProposeOk, ProposeResponse,
};
use crate::plugin::Plugin;
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

/// Test-only constructor exposing private state of `NativeProtocol`
/// so unit tests drive the in-process branches without spawning.
/// Lives here (a child module of `native_participant`) so the prod
/// file stays under the 300-line limit while still reaching the
/// struct's private fields.
impl<'a> NativeProtocol<'a> {
    pub(crate) fn __test_new(
        plugin: &'a Plugin, name: &'a str, event: Event, task: Task, retry_budget: usize,
    ) -> Self {
        Self {
            plugin, name, event, task: Box::new(task),
            accepted: None, pending_conflict: None, retry_budget,
        }
    }
    pub(crate) fn __test_record_ok(&mut self, ok: ProposeOk) { self.accepted = Some(ok); }
    pub(crate) fn __test_record_conflict(&mut self, c: ProposeConflict) {
        self.pending_conflict = Some(c);
    }
    pub(crate) fn __test_task_title(&self) -> String { self.task.title.clone() }
    pub(crate) fn __test_has_pending_conflict(&self) -> bool {
        self.pending_conflict.is_some()
    }
}

pub(crate) fn __test_classify(
    resp: ProposeResponse, accepted: &mut Option<ProposeOk>,
    pending_conflict: &mut Option<ProposeConflict>, name: &str,
) -> AttemptClass {
    classify(resp, accepted, pending_conflict, name)
}
