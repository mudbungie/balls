//! Apply-time helper coverage for the bl-8b71 projection-aware
//! overlay. Integration tests under `tests/plugin_native_protocol.rs`
//! exercise the happy paths end-to-end; this file pins the edge
//! branches the integration scripts can't reach (non-object native
//! payload, no-op merge errors).

use super::*;
use crate::participant::{Field, Projection};
use crate::task::{NewTaskOpts, Task};
use serde_json::json;
use std::collections::BTreeSet;

fn dummy_task(id: &str) -> Task {
    Task::new(
        NewTaskOpts {
            title: "orig".into(),
            ..NewTaskOpts::default()
        },
        id.into(),
    )
}

#[test]
fn payload_as_map_collapses_non_object_native_to_empty_map() {
    let map = payload_as_map(
        "jira",
        &ContributionPayload::Native(serde_json::Value::String("nope".into())),
    );
    assert!(map.is_empty());
}

#[test]
fn payload_as_map_passes_through_object_native_payloads() {
    let map = payload_as_map(
        "jira",
        &ContributionPayload::Native(json!({ "external": { "jira": { "k": "v" } } })),
    );
    assert!(map.contains_key("external"));
}

#[test]
fn project_overlay_owns_field_copies_into_task() {
    let mut task = dummy_task("bl-1234");
    let payload =
        json!({ "title": "from-plugin", "ignored_field": "drop me" })
            .as_object()
            .unwrap()
            .clone();
    let mut owns = BTreeSet::new();
    owns.insert(Field::Title);
    let projection = Projection {
        owns,
        ..Projection::default()
    };
    project_overlay(&mut task, &payload, &projection);
    assert_eq!(task.title, "from-plugin");
}

#[test]
fn project_overlay_no_op_when_payload_lacks_external_key() {
    // External-prefixes projection but no `external` key in payload —
    // the apply path takes the early-return branch inside the
    // external loop. The Task's external map is unchanged.
    let mut task = dummy_task("bl-2345");
    task.external
        .insert("legacy".into(), json!({ "preserved": true }));
    let payload = json!({ "title": "ignored" }).as_object().unwrap().clone();
    let projection = Projection::external_only("jira");
    project_overlay(&mut task, &payload, &projection);
    assert_eq!(task.external["legacy"], json!({ "preserved": true }));
    assert!(!task.external.contains_key("jira"));
}
