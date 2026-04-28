mod diag;
mod dispatch;
mod limits;
mod native_participant;
mod native_types;
mod participant;
mod runner;
mod types;

pub use dispatch::{dispatch_push, dispatch_sync};
pub use native_participant::{NativeOutcome, NativePluginParticipant};
pub use native_types::{
    CommitPolicyWire, DescribeResponse, ProjectionWire, ProposeConflict, ProposeOk,
    ProposeResponse,
};
pub use participant::{LegacyOutcome, LegacyPluginParticipant};
pub use runner::Plugin;
pub use types::{PushResponse, SyncCreate, SyncDelete, SyncReport, SyncUpdate};

use crate::commit_policy::{plan, Contribution, PlanOp};
use crate::error::Result;
use crate::negotiation::{CommitPolicy, FailurePolicy};
use crate::participant::Projection;
use crate::store::{task_lock, Store};
use crate::task::Task;
use chrono::Utc;
use serde_json::{Map, Value};

/// State delta a participant carries through the apply-time planner.
/// Legacy plugins return a verbatim `external.<name>` map; native
/// plugins (bl-8b71) return a partial Task projection. Both are
/// applied through the same projection-aware overlay so concurrent
/// contributions from disjoint plugins compose without clobbering.
pub enum ContributionPayload {
    Legacy(PushResponse),
    Native(Value),
}

/// One participant's contribution to a single push event. The
/// `projection` is what the apply step uses to scope each plugin's
/// payload — a Jira plugin's projection covers `external.jira.*`,
/// and the applier copies only that slice from the payload onto the
/// working task. Disjoint projections compose; overlapping ones are
/// rejected at SPEC §17.3 registration time (out of scope here).
pub struct PushContribution {
    pub name: String,
    pub projection: Projection,
    pub payload: ContributionPayload,
    pub failure_policy: FailurePolicy,
    pub commit_policy: CommitPolicy,
}

/// Apply a set of push contributions and write the state-branch
/// commits implied by their `CommitPolicy`s. Today's all-legacy
/// configs produce one trailing default commit so the observable
/// result is byte-identical to the pre-bl-8b71 dispatcher. Native
/// plugins fold their projected Task slice into the working task at
/// `Apply` time; the planner sequences both kinds together.
pub fn apply_push_contributions(
    store: &Store,
    task_id: &str,
    contributions: &[PushContribution],
) -> Result<()> {
    if contributions.is_empty() {
        return Ok(());
    }
    let plan_steps = plan(
        &contributions
            .iter()
            .map(|c| Contribution {
                name: c.name.clone(),
                failure_policy: c.failure_policy,
                commit_policy: c.commit_policy.clone(),
            })
            .collect::<Vec<_>>(),
        &format!("balls: update external for {task_id}"),
    )?;
    let _g = task_lock(store, task_id)?;
    let mut task = store.load_task(task_id)?;
    let now = Utc::now();
    for op in plan_steps {
        match op {
            PlanOp::Apply(i) => {
                apply_one(&mut task, &contributions[i], now);
                store.save_task(&task)?;
            }
            PlanOp::Commit(msg) => {
                store.commit_task(task_id, &msg)?;
            }
        }
    }
    Ok(())
}

fn apply_one(task: &mut Task, c: &PushContribution, now: chrono::DateTime<chrono::Utc>) {
    let payload_obj = payload_as_map(&c.name, &c.payload);
    project_overlay(task, &payload_obj, &c.projection);
    task.synced_at.insert(c.name.clone(), now);
    task.touch();
}

/// Lift a `ContributionPayload` into a Task-shaped JSON object so the
/// projection-aware overlay treats both protocols the same way.
/// Legacy responses are wrapped under `external.<name>`; native
/// payloads are passed through if they're already objects.
fn payload_as_map(name: &str, payload: &ContributionPayload) -> Map<String, Value> {
    match payload {
        ContributionPayload::Legacy(resp) => {
            let mut external = Map::new();
            external.insert(name.to_string(), Value::Object(resp.0.clone()));
            let mut root = Map::new();
            root.insert("external".into(), Value::Object(external));
            root
        }
        ContributionPayload::Native(v) => match v {
            Value::Object(map) => map.clone(),
            _ => Map::new(),
        },
    }
}

/// Overlay `payload`'s slices onto `task`, restricted to the
/// participant's projection. Owned canonical fields are shallow-
/// copied via a serde round-trip (so unknown projected fields land
/// in `Task::extra` per the bl-d31c forward-compat contract); each
/// declared `external_prefix` updates only its own `external.<prefix>`
/// slot so two disjoint plugins do not clobber each other. Unknown
/// keys outside the projection are dropped. A `from_value` failure
/// on the merged shape is silently dropped — the working task is
/// left as-is, and the caller's failure policy decides what happens.
fn project_overlay(task: &mut Task, payload: &Map<String, Value>, projection: &Projection) {
    if !projection.owns.is_empty() {
        let mut current = task_to_object(task);
        for field in &projection.owns {
            let key = crate::plugin::native_types::field_wire_name(*field);
            if let Some(v) = payload.get(key) {
                current.insert(key.to_string(), v.clone());
            }
        }
        if let Ok(merged) = serde_json::from_value::<Task>(Value::Object(current)) {
            *task = merged;
        }
    }
    if let Some(Value::Object(payload_external)) = payload.get("external") {
        for prefix in &projection.external_prefixes {
            if let Some(slice) = payload_external.get(prefix) {
                task.external.insert(prefix.clone(), slice.clone());
            }
        }
    }
}

/// Serialize `task` into a JSON object. Task always serializes as a
/// struct (= JSON object); both unwraps below are infallible for
/// every state the rest of `bl` produces. Any other shape here is a
/// Task::Serialize bug we want loud rather than silent.
fn task_to_object(task: &Task) -> Map<String, Value> {
    serde_json::to_value(task).unwrap().as_object().unwrap().clone()
}

#[cfg(test)]
#[path = "mod_apply_tests.rs"]
mod apply_tests;
