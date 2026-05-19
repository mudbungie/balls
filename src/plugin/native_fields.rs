//! Canonical `Field` ⇄ wire-string bijection for the native plugin
//! protocol. Split out of `native_types` so the wire *schemas* there
//! stay separate from the *field-naming* contract. The two functions
//! below are exact inverses — keeping them adjacent makes that
//! invariant auditable at a glance. Re-exported from `native_types`
//! so existing `native_types::{field_wire_name, parse_field}` paths
//! are unchanged.

use crate::error::{BallError, Result};
use crate::participant::Field;

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
