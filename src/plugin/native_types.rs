//! Wire schemas for the native plugin protocol (SPEC §5, §8, §10).
//!
//! A plugin opts into native participation by shipping two extra
//! subcommands: `describe` (returns the projection and event
//! subscriptions) and `propose` (per-event negotiation step). Plugins
//! that lack them stay on the legacy push/sync shim — see SPEC §12.
//!
//! Types here are deserialize-only: balls reads the plugin's stdout.
//! The runner (`super::native_runner`) parses these shapes and the
//! participant adapter (`super::native_participant`) converts them
//! into the canonical `Projection` / `Event` / `CommitPolicy` types.

use crate::error::{BallError, Result};
use crate::negotiation::CommitPolicy;
use crate::participant::{Event, Field, Projection};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// Output of `balls-plugin-<name> describe`. The plugin declares the
/// events it subscribes to and the projection of `Task` it claims as
/// authoritative.
#[derive(Debug, Clone, Deserialize)]
pub struct DescribeResponse {
    #[serde(default)]
    pub subscriptions: Vec<Event>,
    pub projection: ProjectionWire,
    /// Optional retry budget override; SPEC §7 default is 5.
    #[serde(default)]
    pub retry_budget: Option<usize>,
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

/// Output of `balls-plugin-<name> propose --event <event>` — exactly
/// one of `ok` or `conflict` is populated. A plugin that returns
/// neither (empty stdout, exit 0) is treated as `Other` — no
/// recoverable conflict, no successful proposal.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeResponse {
    #[serde(default)]
    pub ok: Option<ProposeOk>,
    #[serde(default)]
    pub conflict: Option<ProposeConflict>,
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

/// Inverse of `parse_field` — the on-wire JSON key for a `Field`.
/// Stable: changing one of these renames a Task field on the wire.
pub fn field_wire_name(f: Field) -> &'static str {
    match f {
        Field::Title => "title",
        Field::Type => "type",
        Field::Priority => "priority",
        Field::Status => "status",
        Field::Parent => "parent",
        Field::DependsOn => "depends_on",
        Field::Description => "description",
        Field::Tags => "tags",
        Field::Notes => "notes",
        Field::Links => "links",
        Field::ClaimedBy => "claimed_by",
        Field::Branch => "branch",
        Field::ClosedAt => "closed_at",
        Field::UpdatedAt => "updated_at",
        Field::ClosedChildren => "closed_children",
        Field::External => "external",
        Field::SyncedAt => "synced_at",
        Field::DeliveredIn => "delivered_in",
    }
}

/// Parse a canonical-field string into the `Field` enum. Unknown
/// names are rejected so a typo in `describe` fails registration
/// loudly rather than silently dropping the projection.
pub fn parse_field(name: &str) -> Result<Field> {
    Ok(match name {
        "title" => Field::Title,
        "type" => Field::Type,
        "priority" => Field::Priority,
        "status" => Field::Status,
        "parent" => Field::Parent,
        "depends_on" => Field::DependsOn,
        "description" => Field::Description,
        "tags" => Field::Tags,
        "notes" => Field::Notes,
        "links" => Field::Links,
        "claimed_by" => Field::ClaimedBy,
        "branch" => Field::Branch,
        "closed_at" => Field::ClosedAt,
        "updated_at" => Field::UpdatedAt,
        "closed_children" => Field::ClosedChildren,
        "external" => Field::External,
        "synced_at" => Field::SyncedAt,
        "delivered_in" => Field::DeliveredIn,
        other => {
            return Err(BallError::Other(format!(
                "unknown projection field: {other}"
            )))
        }
    })
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
