//! Field types shared across `project.json`, `repo.json`, and
//! `clone.json` per [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md)
//! §6.5. These are the "layered fields" — `repo.json` is the
//! primary owner of each; `project.json` may carry a project-wide
//! default; `clone.json` may carry a per-on-disk-checkout override.
//! The precedence rule (`clone ?? repo ?? project ?? built-in
//! default`) is resolved in [`crate::effective_config`].
//!
//! Two renames from the pre-XDG schema (SPEC §6.6) land here:
//!
//! - `delivery.mode` → `integrate.mode`. Values
//!   `local-squash` / `deferred` → `direct` / `forge-pr`. The new
//!   name reads what `bl review` is doing — integrating the work
//!   branch into the integration branch.
//! - `review.pre_check` → `review.gate_command`. The new name says
//!   what the field *does* (it gates the review transition) and
//!   that its value is a shell command.
//!
//! The legacy types (`config_blocks::Delivery`, `ReviewConfig`)
//! stay where they are — they belong to the bl-fc50-and-prior
//! schema, and the dual-read path in `Store::discover` continues to
//! consult them when it detects a legacy layout. The new schema
//! uses the types here.

use serde::{Deserialize, Serialize};

/// `integrate.mode` — replaces the pre-XDG `delivery.mode` (SPEC
/// §6.6). `direct` squashes locally now; `forge-pr` hands the
/// squash off to a forge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrateMode {
    #[default]
    Direct,
    ForgePr,
}

/// `integrate` block — its own struct (rather than a bare
/// `IntegrateMode` field) so future integrate knobs extend it
/// without another top-level key. Mirrors the pre-XDG
/// `Delivery` shape, just renamed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integrate {
    #[serde(default)]
    pub mode: IntegrateMode,
}

/// `review` block — the post-XDG name for what was
/// `config_blocks::ReviewConfig`. Holds `gate_command` (the
/// renamed `pre_check`). `None` disables the gate, byte-identical
/// to the absent-block behaviour of the legacy schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewBlock {
    /// Shell command `bl review` runs in the worktree after the
    /// integration branch has been merged in and *before* the
    /// squash. A non-zero exit aborts the review — no squash, no
    /// push, no status flip — so the project's quality gate
    /// (`make check`, clippy, line cap, coverage) is enforced at
    /// the merge, not just in CI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_command: Option<String>,
}

#[cfg(test)]
#[path = "layered_fields_tests.rs"]
mod tests;
