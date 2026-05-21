//! Nested config blocks for `.balls/config.json` — the small structs
//! and enums that hang off `Config`. Split out of `config.rs` (bl-1f38)
//! so adding a block can't push that file past the 300-line cap; the
//! `config` module re-exports every type here, so `config::Delivery`
//! and friends stay byte-identical for callers.

use serde::{Deserialize, Serialize};

/// Which `bl review` code path a repo follows (SPEC §5).
///
/// `LocalSquash` is today's behavior — squash the work branch into the
/// integration branch locally and immediately. `Deferred` hands the
/// squash off to an external forge: `bl review` pushes the work branch
/// and opens an auto-gate instead of touching the integration branch.
/// Absent config ⇒ `LocalSquash`, byte-identical to before this field.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryMode {
    #[default]
    LocalSquash,
    Deferred,
}

/// The `delivery` config block. Its own struct (rather than a bare
/// `DeliveryMode` field) so future delivery knobs extend it without
/// another top-level key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    #[serde(default)]
    pub mode: DeliveryMode,
}

/// The `review` config block (bl-1f38). Its own struct, mirroring
/// `Delivery`, so future review knobs extend it without another
/// top-level key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    /// Shell command `bl review` runs in the worktree after the
    /// integration branch has been merged in and *before* the squash.
    /// A non-zero exit aborts the review — no squash, no push, no
    /// status flip — so the project's quality gate (`make check`,
    /// clippy, line cap, coverage) is enforced at the merge, not just
    /// in CI. `None` (the default, and every config written before
    /// this field) ⇒ no gate, byte-identical to before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_check: Option<String>,
}
