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

/// §7 binding — where the op is happening. `store`/`landing` are the two checkout
/// paths (§1 — the `tasks_branch` store and the `balls/config` landing);
/// `tasks_branch` names the store branch (§4); `invocation_path` is where `bl`
/// was invoked (the project-repo root the delivery plugin needs, §11). `remote`
/// is absent in a no-remote (stealth) repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Binding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    pub tasks_branch: String,
    pub store: String,
    pub landing: String,
    pub invocation_path: String,
}

/// §7 command — the op plus its intended body. `body_change` is the new markdown
/// body when the op rewrites it. There is no field-level changeset on the wire:
/// the op's field diff has one authoritative home — the change worktree plus the
/// op-start state to diff against (§14 derive-don't-store; bl-3bfd, §15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Command {
    pub op: String,
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
    /// There is no after-state companion: a `post` reactor derives the LANDED
    /// ball from git (the §14 derive-don't-store rule), never the wire — so §7
    /// carries no post `current_state` (bl-667e, §15).
    pub before: Option<Task>,
}

/// Post facts threaded onto a `post`/`rollback` payload: the new commit, the tip
/// it landed on, and the §5 trailers parsed from the seal message (incl. the
/// now-assigned `bl-id`). `None` of these is on a `pre` payload — the id is not
/// sealed yet (§7). `metadata` is `None` on a diffless op (§13): sync/prime
/// author no §5 message, so the wire carries the commit pair alone.
#[derive(Debug, Clone, Copy)]
pub struct SealFacts<'a> {
    pub commit: &'a str,
    pub previous_commit: &'a str,
    pub metadata: Option<&'a Metadata>,
}

/// One fully-assembled §7 payload, serialized to a plugin's stdin. Built by
/// [`OpContext::wire`]; `serde` omits every `None`, so the same struct renders
/// the `pre` shape (states only), the `post` shape (+ commits, + metadata on a
/// sealing op, commits-only on a diffless one — §13), and a `rollback` shape
/// (+ `rolling_back`).
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
        Self { actor, binding, command: None, before: None }
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
            // post shape: the op-start state slides from current_state into
            // previous_state, and the commit pair lands. There is NO post
            // current_state — the landed ball is derived from git, not the wire
            // (§14 derive-don't-store; bl-667e, §15). `metadata` follows only when
            // the op sealed a §5 message — a diffless op (§13) carries the commit
            // pair alone.
            p.current_state = None;
            p.previous_state = self.before.as_ref();
            p.commit = Some(s.commit);
            p.previous_commit = Some(s.previous_commit);
            p.metadata = s.metadata;
        }
        p
    }

    /// Stamp the §6 READ-OP payload for `plugin_name`: the §7 wire minus the
    /// task-op fields — no `command`, no states, no commit pair (a read authors
    /// and seals nothing) — in the single phase `"read"` (a read has no
    /// `pre`/`post` split). `metadata` carries the named ball's `bl-id`, the same
    /// id channel a sealed post wire uses; there is still no return channel —
    /// balls folds the plugin's stdout into the HUMAN render verbatim and parses
    /// nothing back (§6/§7).
    #[must_use]
    pub fn read_wire<'a>(&'a self, plugin_name: &'a str, op: &'a str, metadata: &'a Metadata) -> Payload<'a> {
        Payload {
            protocol: crate::message::PROTOCOL,
            op,
            phase: "read",
            plugin_name,
            actor: &self.actor,
            binding: &self.binding,
            command: None,
            current_state: None,
            previous_state: None,
            commit: None,
            previous_commit: None,
            metadata: Some(metadata),
            rolling_back: None,
        }
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
