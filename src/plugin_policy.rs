//! `bl plugin policy` / `bl plugin show` — edit and inspect SPEC §11
//! per-event participant policy (bl-5cc2). bl-32e5's
//! enable/disable/list left the `participant` block read-only; this
//! module makes the per-event `required`/`best-effort`/`gating` knobs
//! first-class and, crucially, makes the `None` vs explicit-empty
//! `{}` distinction expressible without hand-editing JSON: `None`
//! falls through to the legacy `sync_on_change` mapping, an explicit
//! empty map suppresses that fallback (the plugin participates in
//! nothing).
//!
//! Writes land in the same effective config `plugin_admin` mutates —
//! `.balls/project.json` on the tracker branch (SPEC §7), inherited by
//! every clone. There is deliberately no per-clone `--local`
//! surface: a clone overriding the project's plugin policy locally
//! is the exact drift project-owned config exists to prevent.

use crate::config::PluginEntry;
use crate::error::{BallError, Result};
use crate::participant::Event;
use crate::participant_config::{legacy_subscriptions, EventPolicy, ParticipantConfig, PolicyKind};
use crate::plugin_admin;
use crate::store::Store;
use std::path::PathBuf;

/// One participant-policy mutation. The four variants are mutually
/// exclusive — [`parse_op`] builds exactly one and the CLI ArgGroup
/// rejects combinations at the command boundary.
#[derive(Debug, PartialEq, Eq)]
pub enum PolicyOp {
    /// Upsert each `(event, kind)` into the participant block,
    /// materializing the block if the plugin had none.
    Set(Vec<(Event, PolicyKind)>),
    /// Drop each event's subscription. Requires an existing block;
    /// the block itself stays present (possibly empty) — only
    /// [`PolicyOp::Clear`] removes it.
    Rm(Vec<Event>),
    /// Remove the participant block entirely: the plugin falls back
    /// to the legacy `sync_on_change` mapping.
    Clear,
    /// Write an explicit empty subscriptions map: suppresses the
    /// legacy fallback so the plugin participates in nothing.
    NoLegacy,
}

/// Outcome of [`apply`], carrying what the command layer needs to
/// print the follow-up hint.
#[derive(Debug)]
pub struct PolicyReport {
    pub config_path: PathBuf,
}

/// One plugin's effective entry plus its resolved per-event policy,
/// for `bl plugin show`.
#[derive(Debug)]
pub struct PluginView {
    pub entry: PluginEntry,
    /// `true` when the entry carries an explicit `participant` block;
    /// `false` when `resolved` was derived from the legacy mapping.
    pub explicit: bool,
    /// Effective repo-level subscriptions: the explicit block when
    /// present, else the legacy `sync_on_change` projection.
    pub resolved: ParticipantConfig,
}

/// Stable lowercase token for an event — both the JSON object key and
/// the CLI token. Exhaustive: a new `Event` variant fails to compile
/// here until it is given a token.
pub fn event_name(event: Event) -> &'static str {
    match event {
        Event::Claim => "claim",
        Event::Review => "review",
        Event::Close => "close",
        Event::Update => "update",
        Event::Sync => "sync",
        Event::Create => "create",
        Event::Drop => "drop",
    }
}

/// Stable token for a failure policy — the CLI `KIND` and the value
/// shown by `bl plugin show`.
pub fn kind_name(kind: PolicyKind) -> &'static str {
    match kind {
        PolicyKind::Required => "required",
        PolicyKind::BestEffort => "best-effort",
        PolicyKind::Gating => "gating",
    }
}

const EVENT_TOKENS: &str = "claim, review, close, update, sync, create, drop";
const KIND_TOKENS: &str = "required, best-effort, gating";

fn parse_event(tok: &str) -> Result<Event> {
    [
        Event::Claim,
        Event::Review,
        Event::Close,
        Event::Update,
        Event::Sync,
        Event::Create,
        Event::Drop,
    ]
    .into_iter()
    .find(|e| event_name(*e) == tok)
    .ok_or_else(|| {
        BallError::Other(format!(
            "unknown event {tok:?}; valid events: {EVENT_TOKENS}"
        ))
    })
}

fn parse_kind(tok: &str) -> Result<PolicyKind> {
    [PolicyKind::Required, PolicyKind::BestEffort, PolicyKind::Gating]
        .into_iter()
        .find(|k| kind_name(*k) == tok)
        .ok_or_else(|| {
            BallError::Other(format!(
                "unknown policy {tok:?}; valid kinds: {KIND_TOKENS}"
            ))
        })
}

fn parse_set_token(tok: &str) -> Result<(Event, PolicyKind)> {
    let (event, kind) = tok.split_once('=').ok_or_else(|| {
        BallError::Other(format!(
            "expected EVENT=KIND, got {tok:?} (e.g. `create=required`)"
        ))
    })?;
    Ok((parse_event(event)?, parse_kind(kind)?))
}

/// Build the single `PolicyOp` from the CLI's raw, already-mutually-
/// exclusive arguments. Token parsing and its error messages live
/// here so the command layer only does I/O.
pub fn parse_op(set: &[String], rm: &[String], clear: bool, no_legacy: bool) -> Result<PolicyOp> {
    if clear {
        return Ok(PolicyOp::Clear);
    }
    if no_legacy {
        return Ok(PolicyOp::NoLegacy);
    }
    if !rm.is_empty() {
        let events = rm.iter().map(|t| parse_event(t)).collect::<Result<Vec<_>>>()?;
        return Ok(PolicyOp::Rm(events));
    }
    if !set.is_empty() {
        let pairs = set
            .iter()
            .map(|t| parse_set_token(t))
            .collect::<Result<Vec<_>>>()?;
        return Ok(PolicyOp::Set(pairs));
    }
    Err(BallError::Other(
        "nothing to do: pass EVENT=KIND tokens, --rm EVENT, --clear, or --no-legacy".into(),
    ))
}

/// Apply `op` to `name`'s entry in the effective config. Re-runs
/// `ProjectConfig::validate` so the SPEC §6.2 `drop`-is-observe-only
/// rule fails here, at command time, rather than at the next load;
/// `commit_change` then publishes `project.json` on the tracker branch.
pub fn apply(store: &Store, name: &str, op: PolicyOp) -> Result<PolicyReport> {
    plugin_admin::validate_name(name)?;
    let cfg_path = plugin_admin::effective_config_path(store);
    let mut cfg = plugin_admin::load_or_default(&cfg_path)?;
    let entry = cfg.plugins.get_mut(name).ok_or_else(|| {
        BallError::Other(format!(
            "no plugin named {name:?} in the effective config; \
             run `bl plugin enable {name}` first"
        ))
    })?;
    apply_to_entry(entry, op)?;
    cfg.validate()?;
    cfg.save(&cfg_path)?;
    plugin_admin::commit_change(store, &format!("balls: plugin policy {name}"))?;
    Ok(PolicyReport { config_path: cfg_path })
}

/// The pure mutation, split out so it is unit-testable without a
/// `Store`. `Set` materializes the block; `Rm` requires it to exist.
fn apply_to_entry(entry: &mut PluginEntry, op: PolicyOp) -> Result<()> {
    match op {
        PolicyOp::Clear => entry.participant = None,
        PolicyOp::NoLegacy => entry.participant = Some(ParticipantConfig::default()),
        PolicyOp::Set(pairs) => {
            let mut block = entry.participant.take().unwrap_or_default();
            for (event, kind) in pairs {
                block.subscriptions.insert(event, EventPolicy::new(kind));
            }
            entry.participant = Some(block);
        }
        PolicyOp::Rm(events) => {
            let Some(block) = entry.participant.as_mut() else {
                return Err(BallError::Other(
                    "no participant block to remove subscriptions from: this plugin uses \
                     the legacy `sync_on_change` mapping. Set explicit policy first with \
                     `bl plugin policy <name> <event>=<kind>`"
                        .into(),
                ));
            };
            for event in events {
                block.subscriptions.remove(&event);
            }
        }
    }
    Ok(())
}

/// Gather one plugin's effective entry and resolved per-event policy
/// for `bl plugin show`.
pub fn describe(store: &Store, name: &str) -> Result<PluginView> {
    plugin_admin::validate_name(name)?;
    let plugins = plugin_admin::load_effective(store)?;
    let entry = plugins.get(name).cloned().ok_or_else(|| {
        BallError::Other(format!("no plugin named {name:?} in the effective config"))
    })?;
    let explicit = entry.participant.is_some();
    let resolved = entry
        .participant
        .clone()
        .unwrap_or_else(|| legacy_subscriptions(entry.sync_on_change));
    Ok(PluginView { entry, explicit, resolved })
}

#[cfg(test)]
#[path = "plugin_policy_tests.rs"]
mod tests;
