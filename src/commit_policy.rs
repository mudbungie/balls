//! Apply-time composition for `CommitPolicy` outcomes. SPEC §10.
//!
//! A dispatcher that has just collected a set of participant outcomes
//! for one lifecycle event hands the contributions here. `plan` walks
//! them once and returns an ordered `Vec<PlanOp>` of apply/commit
//! steps. The dispatcher then executes the plan against its store —
//! `Apply(i)` writes participant `i`'s state delta to disk; `Commit`
//! issues a state-branch commit. Splitting "decide" from "execute"
//! keeps the composition logic pure (no git, no I/O), so the
//! interesting cases — required + Suppress error, batch coalescing,
//! plugin-message prefixing — are unit-testable.
//!
//! Per SPEC §10, the rules the planner enforces:
//! - `Suppress` on a `Required` participant is rejected before any
//!   `PlanOp` is emitted; nothing should land on disk.
//! - `Commit { message: Some(body) }` emits its own commit
//!   immediately after the participant applies, with a
//!   `plugin: <name>: ` safety prefix on the title.
//! - `Commit { message: None }` and `Suppress` defer to a trailing
//!   default commit so audit attribution stays on the dispatcher.
//! - `Batch { tag }` defers and coalesces with other participants of
//!   the same tag into one end-of-event commit. A batch flush also
//!   commits any deferred `Suppress`/`Commit { None }` state, so the
//!   trailing default commit is suppressed when any batch fires.

use crate::error::{BallError, Result};
use crate::negotiation::{CommitPolicy, FailurePolicy};
use std::collections::BTreeMap;

/// One participant's contribution to compose. The dispatcher owns the
/// state delta; the planner only needs the metadata.
#[derive(Debug, Clone)]
pub struct Contribution {
    pub name: String,
    pub failure_policy: FailurePolicy,
    pub commit_policy: CommitPolicy,
}

/// One operation in the apply plan. Indices into the original
/// contributions slice; the dispatcher knows what state delta each
/// participant carries.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanOp {
    /// Save the participant's state delta to the working tree.
    Apply(usize),
    /// Issue a state-branch commit with the given message.
    Commit(String),
}

/// Compose contributions into an apply/commit sequence. `default_msg`
/// is the title used for the trailing fallback commit — typically
/// today's `balls: update external for <id>` message so the legacy
/// path is byte-identical when every participant returns the default
/// `CommitPolicy`. Pure: no git, no I/O.
pub fn plan(contributions: &[Contribution], default_msg: &str) -> Result<Vec<PlanOp>> {
    for c in contributions {
        if matches!(c.commit_policy, CommitPolicy::Suppress)
            && c.failure_policy == FailurePolicy::Required
        {
            return Err(BallError::Other(format!(
                "participant {} returned CommitPolicy::Suppress but is Required for this event; \
                 a required outcome must be durable",
                c.name
            )));
        }
    }
    let mut ops = Vec::new();
    let mut batches: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // True iff some saved state has not yet been written to a commit.
    // SPEC §10: any subsequent Commit captures it; otherwise a
    // trailing default or batch commit picks it up at end-of-event.
    let mut deferred_state = false;
    for (i, c) in contributions.iter().enumerate() {
        ops.push(PlanOp::Apply(i));
        match &c.commit_policy {
            CommitPolicy::Commit { message: Some(body) } => {
                ops.push(PlanOp::Commit(plugin_commit_message(&c.name, body)));
                deferred_state = false;
            }
            CommitPolicy::Commit { message: None } | CommitPolicy::Suppress => {
                deferred_state = true;
            }
            CommitPolicy::Batch { tag } => {
                batches.entry(tag.clone()).or_default().push(c.name.clone());
                deferred_state = true;
            }
        }
    }
    let any_batch = !batches.is_empty();
    for (tag, names) in batches {
        ops.push(PlanOp::Commit(batch_commit_message(&tag, &names)));
    }
    if deferred_state && !any_batch {
        ops.push(PlanOp::Commit(default_msg.to_string()));
    }
    Ok(ops)
}

/// Wrap a plugin-supplied commit message with the `plugin: <name>: `
/// safety prefix. The plugin's first line becomes the commit title
/// (with the prefix prepended); any remaining lines become the body.
/// Empty input collapses to the prefix alone, which is awkward but
/// preserves attribution — and a plugin that returns
/// `Some("")` is asking for it.
pub fn plugin_commit_message(name: &str, body: &str) -> String {
    let mut lines = body.split('\n');
    let first = lines.next().unwrap_or("");
    let title = format!("plugin: {name}: {first}");
    let rest: Vec<&str> = lines.collect();
    if rest.is_empty() {
        return title;
    }
    let body = rest.join("\n");
    let body = body.trim_start_matches('\n');
    if body.is_empty() {
        title
    } else {
        format!("{title}\n\n{body}")
    }
}

/// Build the end-of-event commit message for a batched tag. Names are
/// listed in the order participants subscribed (the planner inserts
/// them in iteration order).
pub fn batch_commit_message(tag: &str, names: &[String]) -> String {
    format!("balls: batch {tag}\n\nparticipants: {}", names.join(", "))
}

#[cfg(test)]
#[path = "commit_policy_tests.rs"]
mod tests;
