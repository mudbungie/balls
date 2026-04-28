//! Human-gate participant: stages negotiation outcomes for operator
//! review instead of applying them immediately. SPEC §9 gating policy
//! materialized for the sync event (bl-a46d).
//!
//! When `bl sync --review` is in effect, each plugin's `SyncReport`
//! lands as a JSON file under `.balls/local/pending-sync/{event}/`
//! rather than flowing into `apply_sync_report`. The operator inspects
//! the staged file, then commits it via `bl sync --apply <id>` (which
//! routes back through the normal apply path) or drops it with
//! `bl sync --discard <id>`. Nothing is committed on the state branch
//! during staging — the gate is the *absence* of an apply, not a
//! competing commit.
//!
//! Why a separate module rather than a participant impl: the
//! `Participant` trait is parameterized over a wire `Protocol` whose
//! outcome already exists. The human gate doesn't *do* a wire
//! exchange — it intercepts another participant's outcome and stashes
//! it. That's a dispatcher concern, not a wire one. Keeping the
//! staging primitives here, callable from `cmd_sync` when `--review`
//! is on, leaves the negotiation primitive untouched while still
//! materializing the gating-policy story.
//!
//! Layout under `.balls/local/pending-sync/{event}/{stage_id}.json`:
//!
//! ```json
//! {
//!   "event": "sync",
//!   "plugin": "jira",
//!   "staged_at": "2026-04-28T05:00:00Z",
//!   "report": { ... raw SyncReport ... }
//! }
//! ```
//!
//! Per SPEC §6 every event keys its own subdirectory; only `sync` is
//! populated today, but the on-disk shape is forward-compatible with
//! claim/review/close gating when those land.

use crate::error::{BallError, Result};
use crate::plugin::SyncReport;
use crate::store::Store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const PENDING_DIR: &str = "pending-sync";
const SYNC_EVENT: &str = "sync";

/// One staged negotiation outcome. Today this only carries plugin
/// `SyncReport`s; the variant shape leaves room for claim/review/close
/// outcomes to land under the same staging tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedSync {
    pub event: String,
    pub plugin: String,
    pub staged_at: DateTime<Utc>,
    pub report: SyncReport,
}

/// A staged entry plus the on-disk path it lives at, so callers don't
/// have to recompute the path when discarding or applying.
#[derive(Debug)]
pub struct StagedEntry {
    pub id: String,
    pub path: PathBuf,
    pub data: StagedSync,
}

fn pending_root(store: &Store) -> PathBuf {
    store.local_dir().join(PENDING_DIR)
}

fn pending_dir(store: &Store, event: &str) -> PathBuf {
    pending_root(store).join(event)
}

/// Produce a 4-hex stage id stable for a given (plugin, timestamp)
/// pair. Same generator the task-id path uses so collision behavior is
/// shared. Callers don't depend on this being a UUID — they re-generate
/// on collision.
fn make_stage_id(plugin: &str, ts: DateTime<Utc>) -> String {
    crate::task::Task::generate_id(plugin, ts, 4)
}

/// Stage a plugin SyncReport for human review. Returns the stable id
/// that `apply` and `discard` will accept. The id is derived from
/// (plugin, staged_at); a millisecond bump retries on collision so two
/// stagings in the same run produce distinct files even with low-res
/// clocks.
pub fn stage_sync(store: &Store, plugin: &str, report: &SyncReport) -> Result<String> {
    stage_sync_at(store, plugin, report, Utc::now())
}

/// Clock-injected variant of `stage_sync`. Splitting the wall-clock
/// read out of the loop body makes the collision-retry exhaustion path
/// testable with deterministic timestamps; production callers go
/// through `stage_sync` and never see this directly.
fn stage_sync_at(
    store: &Store,
    plugin: &str,
    report: &SyncReport,
    start: DateTime<Utc>,
) -> Result<String> {
    let dir = pending_dir(store, SYNC_EVENT);
    fs::create_dir_all(&dir)?;
    let mut now = start;
    let mut id = make_stage_id(plugin, now);
    let mut path = dir.join(format!("{id}.json"));
    let mut tries = 0;
    while path.exists() {
        tries += 1;
        if tries > 1000 {
            return Err(BallError::Other(
                "could not allocate unique stage id".into(),
            ));
        }
        now += chrono::Duration::milliseconds(1);
        id = make_stage_id(plugin, now);
        path = dir.join(format!("{id}.json"));
    }
    let payload = StagedSync {
        event: SYNC_EVENT.into(),
        plugin: plugin.into(),
        staged_at: now,
        report: report.clone(),
    };
    fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
    Ok(id)
}

/// Look up a staged entry by id. Searches every event subdir so the
/// caller doesn't need to know which lifecycle event produced the
/// staging — the user types `bl sync --apply <id>` and the file is
/// found wherever it lives.
pub fn load_staged(store: &Store, id: &str) -> Result<StagedEntry> {
    let root = pending_root(store);
    if !root.exists() {
        return Err(BallError::Other(format!("no staged entry: {id}")));
    }
    for entry in fs::read_dir(&root)? {
        let event_dir = entry?.path();
        if !event_dir.is_dir() {
            continue;
        }
        let candidate = event_dir.join(format!("{id}.json"));
        if candidate.exists() {
            let bytes = fs::read(&candidate)?;
            let data: StagedSync = serde_json::from_slice(&bytes)?;
            return Ok(StagedEntry { id: id.into(), path: candidate, data });
        }
    }
    Err(BallError::Other(format!("no staged entry: {id}")))
}

/// List every staged entry across all event dirs in stable id order.
pub fn list_staged(store: &Store) -> Result<Vec<StagedEntry>> {
    let root = pending_root(store);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&root)? {
        let event_dir = entry?.path();
        if !event_dir.is_dir() {
            continue;
        }
        for f in fs::read_dir(&event_dir)? {
            let p = f?.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let id = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let bytes = fs::read(&p)?;
            let data: StagedSync = serde_json::from_slice(&bytes)?;
            out.push(StagedEntry { id, path: p, data });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Drop a staged entry without applying it. The file is removed; no
/// state-branch commit lands. Returns the (event, plugin) the caller
/// can render to the operator.
pub fn discard_staged(store: &Store, id: &str) -> Result<(String, String)> {
    let entry = load_staged(store, id)?;
    fs::remove_file(&entry.path)?;
    Ok((entry.data.event, entry.data.plugin))
}

#[cfg(test)]
#[path = "human_gate_tests.rs"]
mod tests;
