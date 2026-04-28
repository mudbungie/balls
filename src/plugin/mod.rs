mod diag;
mod dispatch;
mod limits;
mod participant;
mod runner;
mod types;

pub use dispatch::{dispatch_push, dispatch_sync};
pub use participant::{LegacyOutcome, LegacyPluginParticipant};
pub use runner::Plugin;
pub use types::{PushResponse, SyncCreate, SyncDelete, SyncReport, SyncUpdate};

use crate::commit_policy::{plan, Contribution, PlanOp};
use crate::error::Result;
use crate::negotiation::{CommitPolicy, FailurePolicy};
use crate::store::{task_lock, Store};
use chrono::Utc;
use serde_json::Value;

/// One legacy plugin's contribution to a single push event: what state
/// to fold into `task.external` plus the metadata the apply-time
/// planner needs (failure policy controls the Required+Suppress
/// validation, commit policy controls how the result lands in git).
pub struct LegacyPushContribution {
    pub name: String,
    pub response: PushResponse,
    pub failure_policy: FailurePolicy,
    pub commit_policy: CommitPolicy,
}

/// Apply a set of push contributions to `task.external` and write the
/// state-branch commits implied by their `CommitPolicy`s. Today every
/// legacy plugin contributes the default policy, so the planner emits
/// one trailing default commit and the observable result is
/// byte-identical to the pre-CommitPolicy dispatcher. Native plugins
/// (bl-8b71) opt into Suppress / Batch / custom messages via this
/// same path.
pub fn apply_push_contributions(
    store: &Store,
    task_id: &str,
    contributions: &[LegacyPushContribution],
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
                let c = &contributions[i];
                task.external
                    .insert(c.name.clone(), Value::Object(c.response.0.clone()));
                task.synced_at.insert(c.name.clone(), now);
                task.touch();
                store.save_task(&task)?;
            }
            PlanOp::Commit(msg) => {
                store.commit_task(task_id, &msg)?;
            }
        }
    }
    Ok(())
}
