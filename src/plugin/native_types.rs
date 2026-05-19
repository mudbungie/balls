//! Wire schemas for the native plugin protocol (SPEC §5, §8, §10).
//!
//! A plugin opts in by shipping `describe` + `propose` subcommands;
//! lacking them it stays on the legacy push/sync shim (SPEC §12).
//! Most types are deserialize-only (balls reads plugin stdout);
//! `EventCtxWire` is the one serialize-only shape — balls writes it
//! to the §5.1 side channel.

use crate::error::{BallError, Result};
use crate::negotiation::CommitPolicy;
use crate::participant::{Event, Field, Projection};
use crate::task::Task;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

// The `Field` ⇄ wire-string bijection lives in `native_fields` to
// keep this file under the 300-line cap. Re-exported so the existing
// `native_types::{field_wire_name, parse_field}` paths are unchanged.
pub use crate::plugin::native_fields::{field_wire_name, parse_field};

/// Output of `balls-plugin-<name> describe`. The plugin declares the
/// events it subscribes to and the projection of `Task` it claims as
/// authoritative.
#[derive(Debug, Clone, Deserialize)]
pub struct DescribeResponse {
    /// SPEC §13 seam 3: parsed leniently. An event this build does
    /// not know is dropped from the set; the known events still
    /// negotiate and the handshake survives. It does NOT hard-error
    /// the describe parse (which would lose the plugin's whole native
    /// protocol to the legacy shim).
    #[serde(default, deserialize_with = "lenient_events")]
    pub subscriptions: Vec<Event>,
    pub projection: ProjectionWire,
    /// Optional retry budget override; SPEC §7 default is 5.
    #[serde(default)]
    pub retry_budget: Option<usize>,
    /// SPEC §5.1 — opt into the describe-gated `EventCtx` side
    /// channel. Absent/false ⇒ the plugin receives byte-identical
    /// input to today (no `--ctx-file`). Additive per §13: an older
    /// `bl` never reads it; an older plugin never sets it.
    #[serde(default)]
    pub wants_context: bool,
}

/// Schema version for the `EventCtx` side channel. Bumped only on a
/// breaking change; new *keys* are additive and do NOT bump it (SPEC
/// §5.1 / §13: a context-aware plugin ignores keys it doesn't know).
pub const EVENT_CTX_SCHEMA_VERSION: u32 = 1;

/// SPEC §5.1 — the `EventCtx` document delivered on the describe-gated
/// side channel (`--ctx-file`). Serialize-only: `bl` writes it, the
/// plugin reads it; the post-image Task still arrives unchanged on
/// stdin. `task_before` (the pre-image diff basis) and `commit` (the
/// state-branch sha of this event) are the reserved additive keys
/// bl-fb4d wires from the command side; both skip-serialize when
/// absent so an old context-aware plugin is unaffected (SPEC §13).
#[derive(Debug, Clone, Serialize)]
pub struct EventCtxWire {
    pub schema_version: u32,
    pub event: String,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    pub overrides: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

impl EventCtxWire {
    /// Build the v1 EventCtx JSON for one event. `task_before` is
    /// serialized as the prior Task projection; `overrides` carries
    /// the per-invocation tokens that applied (SPEC §11).
    pub fn for_event(
        event: &str,
        actor: &str,
        repo: Option<String>,
        task_before: Option<&Task>,
        commit: Option<&str>,
        overrides: &[String],
    ) -> Result<String> {
        let task_before = task_before
            .map(serde_json::to_value)
            .transpose()
            .map_err(BallError::Json)?;
        serde_json::to_string(&Self {
            schema_version: EVENT_CTX_SCHEMA_VERSION,
            event: event.to_string(),
            actor: actor.to_string(),
            repo,
            overrides: overrides.to_vec(),
            task_before,
            commit: commit.map(str::to_string),
        })
        .map_err(BallError::Json)
    }
}

/// Deserialize a describe subscription list, dropping any element this
/// build does not recognize as an `Event`. SPEC §13 seam 3: an older
/// `bl` meeting a plugin that subscribes to a newer event (e.g.
/// `create`) keeps what it understands and drops the rest — unknown =
/// round-trip, never die. A non-event element (wrong type, etc.) is
/// dropped the same way rather than failing the whole handshake.
fn lenient_events<'de, D>(d: D) -> std::result::Result<Vec<Event>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Vec::<Value>::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Event>(v).ok())
        .collect())
}

/// Wire shape for the field projection. Strings are parsed into
/// `Field` at registration so a plugin that returns an unknown name
/// fails loudly instead of silently dropping fields.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectionWire {
    #[serde(default)]
    pub owns: Vec<String>,
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub external_prefixes: Vec<String>,
}

/// Output of `balls-plugin-<name> propose --event <event>` — at most
/// one of `ok`, `conflict`, or `reject` is populated. A plugin that
/// returns none of them (empty stdout, exit 0) is treated as `Other`
/// — no recoverable conflict, no successful proposal, no veto.
///
/// SPEC §8.1: the three branches are distinct and must not be
/// conflated. `ok`/`conflict` carry Task state; `reject` carries a
/// policy veto with a human reason and no state.
///
/// SPEC §13 seam 2: any other top-level key (a variant this build
/// does not know) lands in `extra` instead of being silently
/// discarded. `classify` then degrades to `Other` and names the
/// variant. Capturing rather than dropping is what keeps the rule
/// symmetric with `Task::extra` and robust against a future
/// `deny_unknown_fields`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProposeResponse {
    #[serde(default)]
    pub ok: Option<ProposeOk>,
    #[serde(default)]
    pub conflict: Option<ProposeConflict>,
    #[serde(default)]
    pub reject: Option<ProposeReject>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// SPEC §8.1 — a deliberate policy veto. Carries a human reason and
/// NO Task state. Distinct from `conflict` (recoverable field clash,
/// retried) and `Other` (wire failure). The negotiation primitive
/// maps it by the subscription's failure policy: required aborts the
/// event with this reason; best-effort warns and continues; gating
/// stages. `reason` defaults to empty so a terse `{"reject":{}}`
/// still reads as a veto rather than failing the parse.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProposeReject {
    #[serde(default)]
    pub reason: String,
}

/// Successful propose: the plugin returns a partial Task JSON
/// containing the fields it owns. The framework folds those fields
/// into the working Task by projection at apply time.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeOk {
    #[serde(default)]
    pub task: Value,
    #[serde(default)]
    pub commit_policy: Option<CommitPolicyWire>,
}

/// Recoverable conflict: the plugin saw a remote-side change that
/// invalidates its proposal. `remote_view` is a partial Task JSON the
/// framework folds into the working Task before retrying. SPEC §8.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeConflict {
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub remote_view: Value,
    #[serde(default)]
    pub hint: Option<String>,
}

/// Wire shape for `CommitPolicy`. A native plugin can attach this to
/// its `Ok` response so the apply-time planner schedules its commit
/// per SPEC §10. Discriminated by a `kind` field instead of serde's
/// default `type` to avoid colliding with payload types in tracker
/// JSON.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CommitPolicyWire {
    Commit {
        #[serde(default)]
        message: Option<String>,
    },
    Batch {
        tag: String,
    },
    Suppress,
}

impl CommitPolicyWire {
    pub fn into_policy(self) -> CommitPolicy {
        match self {
            CommitPolicyWire::Commit { message } => CommitPolicy::Commit { message },
            CommitPolicyWire::Batch { tag } => CommitPolicy::Batch { tag },
            CommitPolicyWire::Suppress => CommitPolicy::Suppress,
        }
    }
}

impl ProjectionWire {
    /// Build a typed `Projection` from the wire form. Returns an
    /// error if any string fails `parse_field`.
    pub fn into_projection(self) -> Result<Projection> {
        let owns: BTreeSet<Field> = self
            .owns
            .iter()
            .map(|s| parse_field(s))
            .collect::<Result<_>>()?;
        let reads: BTreeSet<Field> = self
            .reads
            .iter()
            .map(|s| parse_field(s))
            .collect::<Result<_>>()?;
        let external_prefixes: BTreeSet<String> = self.external_prefixes.into_iter().collect();
        Ok(Projection {
            owns,
            reads,
            external_prefixes,
        })
    }
}

#[cfg(test)]
#[path = "native_types_tests.rs"]
mod tests;
