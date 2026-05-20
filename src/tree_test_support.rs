//! Shared task builders for the parent-edge tree renderer tests.
//! Declared at crate level (cfg(test)) so the structural tests
//! (`tree_tests.rs`) and the `format_line` annotation tests
//! (`tree_format_tests.rs`) share one copy.

use crate::task::{NewTaskOpts, Status, Task, TaskType};

pub(crate) fn mk(id: &str, parent: Option<&str>) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: id.into(),
            parent: parent.map(String::from),
            ..Default::default()
        },
        id.into(),
    );
    t.status = Status::Open;
    t
}

pub(crate) fn mk_full(
    id: &str,
    parent: Option<&str>,
    deps: &[&str],
    status: Status,
    ttype: TaskType,
) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: id.into(),
            parent: parent.map(String::from),
            depends_on: deps.iter().map(|s| String::from(*s)).collect(),
            task_type: ttype,
            ..Default::default()
        },
        id.into(),
    );
    t.status = status;
    t
}
