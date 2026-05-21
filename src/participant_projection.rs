//! `Field` and `Projection` — the participant data model (SPEC §3,
//! §5), split out of `participant.rs` to keep it under the 300-line
//! cap. `participant` re-exports both, so `crate::participant::{Field,
//! Projection}` paths elsewhere in the crate are unchanged.

use std::collections::BTreeSet;

/// SPEC §3 — canonical Task field set used by `Projection`. Mirrors
/// the public fields of `task::Task`. `External` is the whole
/// `external` map; per-plugin slices are declared via
/// `Projection::external_prefixes` so two plugins claiming
/// `external.jira.*` and `external.linear.*` don't appear to collide
/// at the field level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Title,
    Type,
    Priority,
    Status,
    Parent,
    DependsOn,
    Description,
    Tags,
    Notes,
    Links,
    ClaimedBy,
    Branch,
    ClosedAt,
    UpdatedAt,
    ClosedChildren,
    External,
    SyncedAt,
    DeliveredIn,
}

impl Field {
    /// Every canonical field. The git-remote participant owns this
    /// set; plugins typically read it.
    pub fn all() -> BTreeSet<Field> {
        [
            Field::Title,
            Field::Type,
            Field::Priority,
            Field::Status,
            Field::Parent,
            Field::DependsOn,
            Field::Description,
            Field::Tags,
            Field::Notes,
            Field::Links,
            Field::ClaimedBy,
            Field::Branch,
            Field::ClosedAt,
            Field::UpdatedAt,
            Field::ClosedChildren,
            Field::External,
            Field::SyncedAt,
            Field::DeliveredIn,
        ]
        .into_iter()
        .collect()
    }
}

/// SPEC §5 — what a participant owns and reads. Disjoint owners
/// compose; overlapping owners on the same event require an explicit
/// merge function (out of scope here — see bl-b1dd, bl-8b71). External
/// slices are declared by prefix, e.g. `jira` -> `external.jira.*`.
/// That prefix-level disjointness is what makes "closed in git, open
/// in Jira" mergeable without a merge function in this ball.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Projection {
    pub owns: BTreeSet<Field>,
    pub reads: BTreeSet<Field>,
    pub external_prefixes: BTreeSet<String>,
}

impl Projection {
    /// Git-remote shape: own every canonical field, read nothing extra.
    pub fn full() -> Self {
        Self {
            owns: Field::all(),
            reads: BTreeSet::new(),
            external_prefixes: BTreeSet::new(),
        }
    }

    /// Plugin shape: own only `external.<name>.*`; read everything
    /// canonical. Used by the legacy shim (bl-b1dd) and as the default
    /// projection for native plugins until they declare richer ones.
    pub fn external_only(prefix: impl Into<String>) -> Self {
        let mut external_prefixes = BTreeSet::new();
        external_prefixes.insert(prefix.into());
        Self {
            owns: BTreeSet::new(),
            reads: Field::all(),
            external_prefixes,
        }
    }

    /// Two projections overlap if they both own the same canonical
    /// field, or the same `external.<prefix>`. SPEC §5 demands an
    /// explicit merge for overlap; until that lands (bl-8b71),
    /// callers reject overlaps at registration time.
    pub fn overlaps(&self, other: &Projection) -> bool {
        self.owns.intersection(&other.owns).next().is_some()
            || self
                .external_prefixes
                .intersection(&other.external_prefixes)
                .next()
                .is_some()
    }
}

#[cfg(test)]
#[path = "participant_projection_tests.rs"]
mod tests;
