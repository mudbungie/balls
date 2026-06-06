//! §7 plugin wire payloads — what balls hands a plugin on stdin.
//!
//! Plugins get meaning directly on the wire — content + intent, not hashes to
//! reverse-engineer. There is **no return channel** (§7): a plugin contributes
//! by editing the change worktree, never by printing values back; its stdout is
//! diagnostics. So these types are *output only* — balls serializes them with
//! `serde_json` and never deserializes a payload (a plugin self-describe is the
//! one thing read back, and that lives in [`crate::plugin`]).
//!
//! One [`OpContext`] holds everything that does not change between a plugin's
//! `pre` and `post` invocation (the verb layer, bl-dfbd, computes it). A `phase`
//! plus the optional post-seal [`SealFacts`] pick the rest: `pre`, `post`, and
//! `rollback` payloads differ only in which fields are present, so a single
//! [`Payload`] with `serde` skipping the `None`s is the whole shape (§7
//! "post payload: same plus …").

use serde::Serialize;

use crate::message::Metadata;
use crate::task::Task;

/// §7 binding — where the op is happening. `operating` is the terminus checkout;
/// `invocation_path` is where `bl` was invoked (the project-repo root the
/// delivery plugin needs, §11). `remote` is absent in a no-remote (stealth) repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Binding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    pub branch: String,
    pub operating: String,
    pub invocation_path: String,
}

/// One field-level intent in a [`Command`]: set `field` to `value`, or clear it
/// when `value` is `None`. The §7 "intended `field_changes`" — content, not a
/// diff hash the plugin has to reverse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    pub field: String,
    pub value: Option<String>,
}

/// §7 command — the op plus its intended diff. `body_change` is the new markdown
/// body when the op rewrites it. `field_changes` empty = a body-only or
/// occupancy-only op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Command {
    pub op: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub field_changes: Vec<FieldChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_change: Option<String>,
}

/// The op-constant §7 wire data: everything identical across a plugin's `pre`
/// and `post` calls. The verb layer authors it once per op; [`OpContext::wire`]
/// stamps a per-phase [`Payload`] from it.
///
/// Not `Eq` (it holds a [`Task`], which is only `PartialEq`).
#[derive(Debug, Clone, PartialEq)]
pub struct OpContext {
    pub actor: String,
    pub binding: Binding,
    /// The op + its intended diff. `None` on a diffless checkout-lifecycle op
    /// (`sync`/`prime`): those author no ball, so the §13 wire omits `command`
    /// entirely — a plugin gets meaning from its `binding`, not a command.
    pub command: Option<Command>,
    /// The ball at op-start, before the base change. `None` on create (no ball
    /// yet). This is `pre`'s `current_state` and `post`'s `previous_state` (§7).
    pub before: Option<Task>,
    /// The ball as sealed, after the change. `None` on close/drop (file gone).
    /// This is `post`'s `current_state`; `pre` never sees it (§7).
    pub after: Option<Task>,
}

/// Post-seal facts threaded onto a `post`/`rollback` payload: the sealed commit,
/// the tip it landed on, and the §5 trailers parsed from the seal message (incl.
/// the now-assigned `bl-id`). `None` of these is on a `pre` payload — the id is
/// not sealed yet (§7).
#[derive(Debug, Clone, Copy)]
pub struct SealFacts<'a> {
    pub commit: &'a str,
    pub previous_commit: &'a str,
    pub metadata: &'a Metadata,
}

/// One fully-assembled §7 payload, serialized to a plugin's stdin. Built by
/// [`OpContext::wire`]; `serde` omits every `None`, so the same struct renders
/// the `pre` shape (states only), the `post` shape (+ commits + metadata), and a
/// `rollback` shape (+ `rolling_back`).
#[derive(Debug, Serialize)]
pub struct Payload<'a> {
    pub protocol: u32,
    pub op: &'a str,
    pub phase: &'a str,
    pub plugin_name: &'a str,
    pub actor: &'a str,
    pub binding: &'a Binding,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<&'a Command>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_state: Option<&'a Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_state: Option<&'a Task>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_commit: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<&'a Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rolling_back: Option<&'a str>,
}

impl OpContext {
    /// The op-constant wire for a DIFFLESS checkout-lifecycle op (`sync`/`prime`,
    /// §13): a plugin gets meaning from its `binding` alone, so `command` and
    /// both task states are absent (these ops author no ball). `actor` is the
    /// invoking identity (`--as`).
    #[must_use]
    pub fn diffless(actor: String, binding: Binding) -> Self {
        Self { actor, binding, command: None, before: None, after: None }
    }

    /// Stamp the §7 payload for `plugin_name` in `phase`. `sealed` is `Some` once
    /// the op has sealed (every `post` call and any `post`-phase rollback),
    /// which selects the after-state and adds the commit facts; `pre` passes
    /// `None`. `rolling_back` is `Some("pre"|"post")` only on a rollback call.
    #[must_use]
    pub fn wire<'a>(
        &'a self,
        plugin_name: &'a str,
        op: &'a str,
        phase: &'a str,
        sealed: Option<SealFacts<'a>>,
        rolling_back: Option<&'a str>,
    ) -> Payload<'a> {
        let mut p = Payload {
            protocol: crate::message::PROTOCOL,
            op,
            phase,
            plugin_name,
            actor: &self.actor,
            binding: &self.binding,
            command: self.command.as_ref(),
            current_state: self.before.as_ref(),
            previous_state: None,
            commit: None,
            previous_commit: None,
            metadata: None,
            rolling_back,
        };
        if let Some(s) = sealed {
            // post shape: current advances to the sealed after-state, the
            // op-start state slides into previous_state, commits + metadata land.
            p.current_state = self.after.as_ref();
            p.previous_state = self.before.as_ref();
            p.commit = Some(s.commit);
            p.previous_commit = Some(s.previous_commit);
            p.metadata = Some(s.metadata);
        }
        p
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
