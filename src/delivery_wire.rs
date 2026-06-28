//! The §7 wire slice the delivery plugin deserializes from stdin — lifted from
//! [`super`] so that file stays the dispatch POLICY (bl-2656 made room for the
//! `close.post` code push). balls only ever serializes the full wire
//! ([`crate::wire`]); the plugin owns the matching deserialize for the slice it
//! needs. Re-exported as `crate::delivery::Wire`, so the binary edge is unchanged.

use serde::Deserialize;

use crate::message::Metadata;

/// The §7 fields the delivery plugin reads off stdin. balls only ever
/// serializes the wire ([`crate::wire`]); the plugin owns the matching
/// deserialize for the slice it needs — `invocation_path` (the project root),
/// the `bl-id` metadata, the ball `title` for the squash subject, and the
/// `rolling_back` tag.
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

/// The one §7 command field the plugin needs: the `-m` `message`, read as the
/// FULL delivery message override (bl-b9a6) when a close carried one.
#[derive(Debug, Default, Deserialize)]
pub struct WireCommand {
    #[serde(default)]
    pub message: Option<String>,
}
