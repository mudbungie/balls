//! §16 legacy read shim — `--legacy[=REF]` on the read verbs (and, through
//! them, on `bl import`). ONE place owns "how to read legacy": the
//! pre-greenfield task JSON (`.balls/tasks/*.json` on the old store branch) is
//! read from a git ref of the PROJECT repo and projected into the greenfield
//! wire shape, so `bl list --legacy` is the migration preview and `bl list
//! --legacy --json | bl import` is the migration itself — no external filter,
//! no legacy binary.
//!
//! The projection is the §16 core field map, PER TASK: `claimed_by`→`claimant`,
//! `created_at`/`updated_at` ISO→i64, `depends_on`→`blockers {id, on: claim}`,
//! `type: epic`→tag, `status: deferred`→tag, `description` + folded
//! notes→body; closed tasks are skipped (file-absent = resolved, §9) and a
//! dangling parent is nulled (migrate-clean-or-delink). It is a RENAME, never a
//! reconstruction: the cross-task epic reciprocal edge is deliberately NOT
//! minted here — `bl import --legacy` wires it through the ordinary `update
//! --needs` machinery (§16), keeping this shim a pure projection.
//!
//! Severable by design (§16): the flag arms, this module, and nothing else —
//! deleting the `--legacy` capability deletes code, not core.

use std::collections::HashSet;
use std::io;
use std::path::Path;

use crate::civil::start_of_day;
use crate::git;
use crate::task::{Blocker, On, Task};
use crate::taskfile::invalid;

/// The default `--legacy` spec — `<ref>:<dir>`: legacy balls kept its JSON
/// store in `.balls/tasks/` on the (colliding, §16) `balls/tasks` branch.
pub(crate) const DEFAULT_SPEC: &str = "balls/tasks:.balls/tasks";

/// Parse a `--legacy`/`--legacy=REF` argv token to its `<ref>:<dir>` spec
/// (`None` ⇒ not the legacy flag). The one owner of the flag spelling, shared
/// by the read parser and `bl import`'s.
pub(crate) fn flag(arg: &str) -> Option<String> {
    if arg == "--legacy" {
        return Some(DEFAULT_SPEC.to_string());
    }
    arg.strip_prefix("--legacy=").map(str::to_string)
}

/// Read every LIVE legacy task from `repo` at `spec` (`<ref>:<dir>`, a bare
/// `<ref>` defaulting the dir) and project it into greenfield `(id, Task)`
/// pairs, id-sorted — exactly the shape [`super::Catalog::from_pairs`] and
/// `bl import` consume.
pub(crate) fn balls(repo: &Path, spec: &str) -> io::Result<Vec<(String, Task)>> {
    let (ref_, dir) = spec.split_once(':').unwrap_or((spec, ".balls/tasks"));
    let listing = git::run(repo, &["ls-tree", "-r", "--name-only", ref_, dir], None)?;
    let mut live = Vec::new();
    for path in listing.lines().filter(|p| Path::new(p).extension().is_some_and(|e| e == "json")) {
        let text = git::run(repo, &["show", &format!("{ref_}:{path}")], None)?;
        let t: Legacy =
            serde_json::from_str(&text).map_err(|e| invalid(format!("legacy {path}: {e}")))?;
        if t.status.as_deref() != Some("closed") {
            live.push(t);
        }
    }
    // Notes are a second read per task, folded into the body by the projection.
    let notes: Vec<String> = live
        .iter()
        .map(|t| git::run(repo, &["show", &format!("{ref_}:{dir}/{}.notes.jsonl", t.id)], None).unwrap_or_default())
        .collect();
    project(&live, &notes)
}

/// One legacy task's JSON, tolerantly typed (legacy wrote explicit `null`s).
/// Only the §16-mapped core fields are read; everything else — `delivered_in`,
/// `branch`, `external.*`, … — has no core home and is dropped here
/// (migrate-clean-or-delink: plugins re-adopt their own territory).
#[derive(serde::Deserialize)]
struct Legacy {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    claimed_by: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    depends_on: Option<Vec<String>>,
    #[serde(default)]
    description: Option<String>,
}

/// The pure §16 projection over the LIVE legacy set (`notes` parallel to
/// `live`): the per-task field map, dangling-parent nulling against the live
/// id-set, and the notes fold. Git-free, so the map is unit-testable.
fn project(live: &[Legacy], notes: &[String]) -> io::Result<Vec<(String, Task)>> {
    let ids: HashSet<&str> = live.iter().map(|t| t.id.as_str()).collect();
    let mut out = Vec::with_capacity(live.len());
    for (t, raw_notes) in live.iter().zip(notes) {
        let mut tags = t.tags.clone().unwrap_or_default();
        for implied in [
            (t.kind.as_deref() == Some("epic")).then_some("epic"),
            (t.status.as_deref() == Some("deferred")).then_some("deferred"),
        ]
        .into_iter()
        .flatten()
        {
            if !tags.iter().any(|x| x == implied) {
                tags.push(implied.to_string());
            }
        }
        let task = Task {
            title: t.title.clone(),
            created: epoch(&t.id, &t.created_at)?,
            updated: epoch(&t.id, &t.updated_at)?,
            claimant: t.claimed_by.clone(),
            // A parent outside the live set is closed or absent — delinked.
            parent: t.parent.clone().filter(|p| ids.contains(p.as_str())),
            priority: t.priority,
            blockers: t
                .depends_on
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|id| Blocker { id, on: On::Claim })
                .collect(),
            tags,
            body: fold_notes(t.description.as_deref().unwrap_or(""), raw_notes),
            ..Task::default()
        };
        out.push((t.id.clone(), task));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Legacy ISO-8601 (`YYYY-MM-DDTHH:MM:SS…`, any sub-second tail) → §3 unix
/// seconds, via the same civil math the display layer uses.
fn epoch(id: &str, iso: &str) -> io::Result<i64> {
    let parsed = (|| {
        let day = start_of_day(iso.get(..10)?)?;
        let t = iso.get(10..)?.strip_prefix('T')?;
        let field = |r: std::ops::Range<usize>| t.get(r)?.parse::<i64>().ok();
        Some(day + field(0..2)? * 3600 + field(3..5)? * 60 + field(6..8)?)
    })();
    parsed.ok_or_else(|| invalid(format!("legacy {id}: bad timestamp '{iso}'")))
}

/// Body = legacy `description` + the task's `notes.jsonl` folded in as a
/// section. Notes have no greenfield core home; the free-form body is the one
/// place that holds prose, so they ride there losslessly instead of dropping
/// design history. An unparseable note line is skipped (delink, never guess).
fn fold_notes(description: &str, raw: &str) -> String {
    let mut body = description.trim_end().to_string();
    let notes: Vec<String> = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .map(|n| {
            let s = |k: &str| n[k].as_str().unwrap_or("").to_string();
            format!("- {} {}: {}", s("ts"), s("author"), s("text"))
        })
        .collect();
    if !notes.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str("## Notes (migrated)\n\n");
        body.push_str(&notes.join("\n"));
        body.push('\n');
    } else if !body.is_empty() {
        body.push('\n');
    }
    body
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
