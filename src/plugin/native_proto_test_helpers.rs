//! Private-state accessors for `NativeProtocol`, used by
//! `native_participant_proto_tests.rs`. Lives as a child module of
//! `native_proto` so it can reach the struct's private fields and the
//! private `classify`; generic store/plugin fixtures shared with the
//! describe tests live in `super::super::native_test_support`.

use super::{classify, NativeProtocol};
use crate::negotiation::AttemptClass;
use crate::participant::Event;
use crate::plugin::native_types::{ProposeConflict, ProposeOk, ProposeResponse};
use crate::plugin::Plugin;
use crate::task::Task;

/// Test-only constructor exposing private state of `NativeProtocol`
/// so unit tests drive the in-process branches without spawning.
impl<'a> NativeProtocol<'a> {
    pub(crate) fn __test_new(
        plugin: &'a Plugin, name: &'a str, event: Event, task: Task, retry_budget: usize,
    ) -> Self {
        NativeProtocol::new(plugin, name, event, task, retry_budget, false, String::new())
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
