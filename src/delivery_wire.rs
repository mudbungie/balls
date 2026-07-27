//! The §7 wire slice the delivery plugin reads off stdin. balls only ever
//! serializes the wire ([`crate::wire`]); the plugin owns the matching
//! deserialize for the slice it needs, split out of [`crate::delivery`] so the
//! policy module holds only behaviour.

use serde::Deserialize;

use crate::message::Metadata;

/// The §7 fields the delivery plugin reads off stdin — `invocation_path` (the
/// project root), the `bl-id` metadata, the ball `title` for the squash subject,
/// and the `rolling_back` tag.
#[derive(Debug, Deserialize)]
pub struct Wire {
    pub binding: WireBinding,
    /// The §7 command — read only for its `-m` note, the delivery message
    /// override (bl-b9a6). Absent on a diffless op, `message` absent without `-m`.
    #[serde(default)]
    pub command: Option<WireCommand>,
    #[serde(default)]
    pub metadata: Option<Metadata>,
    #[serde(default)]
    pub current_state: Option<WireState>,
    #[serde(default)]
    pub rolling_back: Option<String>,
}

/// The one binding field the plugin needs: where `bl` was invoked (§7/§11) —
/// the project-repo root the derived worktree paths hang off.
#[derive(Debug, Deserialize)]
pub struct WireBinding {
    pub invocation_path: String,
}

/// The one ball field the plugin needs: the title, for the squash subject.
#[derive(Debug, Default, Deserialize)]
pub struct WireState {
    #[serde(default)]
    pub title: String,
}

/// The §7 command fields the plugin needs: the ball `id` this op is about (§0
/// obligation 4 — carried on every payload, so no hook re-derives it from the
/// change worktree; bl-a5f3), the `-m` `message`, read as the FULL delivery
/// message override (bl-b9a6) when a close carried one, and the derived
/// delivery `target` (bl-7b71) — the id of the ball whose `work/<id>` ref this
/// op forks from and folds back into. Absent target = the integration branch,
/// which is every flat ball and every payload written before nesting.
#[derive(Debug, Default, Deserialize)]
pub struct WireCommand {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
}

#[cfg(test)]
#[path = "delivery_wire_tests.rs"]
mod tests;
