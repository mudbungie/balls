//! `bl remaster` reconcile/detach core (bl-2057).
//!
//! Recovery must be a first-class verb, not manual git surgery: an
//! unaware `bl init` is only *safe* (bl-8e8f) if there is a
//! non-destructive path back INTO a project afterwards. That path is
//! `reconcile`: adopt the hub's `balls/tasks` history, then replay
//! the tasks only this repo has on top of it, so the local branch
//! becomes a descendant of the hub and a normal `bl sync` can push.
//!
//! Disjoint task files coexist trivially (one file per id). The one
//! hazard is an id *clash* — the same `bl-xxxx` naming two different
//! tasks (independent offline creation). Reconcile renames the local
//! one to a fresh id and rewrites in-repo references to it, so no
//! task is silently merged into another. No CLI or printing here;
//! that is `commands::remaster`.

use crate::error::{BallError, Result};
use crate::store::Store;
use crate::task::Task;
use crate::{git, git_state};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const STATE_BRANCH: &str = "balls/tasks";
const TASKS_REL: &str = ".balls/tasks";

#[derive(Debug, PartialEq, Eq)]
pub enum Reconciled {
    /// The target is already an ancestor of the local state branch —
    /// re-running against the same hub is a no-op fetch.
    AlreadyUpToDate,
    /// The hub history was adopted; `replayed` local-only tasks were
    /// re-applied and `renamed` id-clashing tasks were re-imported
    /// under fresh ids.
    Joined { replayed: usize, renamed: usize },
}

struct LocalTask {
    json: String,
    notes: String,
}

/// Fetch `target`'s `balls/tasks` and reconcile this repo's
/// local-only tasks onto it. Idempotent. See module docs.
pub fn reconcile(store: &Store, target: &str) -> Result<Reconciled> {
    let sd = store.state_worktree_dir();
    if !git::git_has_remote(&sd, target) {
        return Err(BallError::Other(format!(
            "no git remote `{target}`; add it (git remote add {target} <url>) and retry"
        )));
    }
    if !git::git_fetch(&sd, target)? {
        return Err(BallError::Other(format!("could not fetch from `{target}`")));
    }
    if !git_state::has_remote_branch(&sd, target, STATE_BRANCH) {
        return Err(BallError::Other(format!(
            "`{target}` has no {STATE_BRANCH} branch — nothing to join"
        )));
    }
    let target_ref = format!("refs/remotes/{target}/{STATE_BRANCH}");
    let target_sha = git::git_resolve_sha(&sd, &target_ref)?;
    if git::git_is_ancestor(&sd, &target_sha, STATE_BRANCH) {
        return Ok(Reconciled::AlreadyUpToDate);
    }

    let tdir = sd.join(TASKS_REL);
    let local = read_local_tasks(&tdir)?;
    let target_ids = git_state::ls_task_ids(&sd, &target_ref)?;

    let mut replay: Vec<String> = Vec::new();
    let mut clashes: Vec<String> = Vec::new();
    for (id, lt) in &local {
        if target_ids.contains(id) {
            let theirs = git_state::show_file(&sd, &target_ref, &json_rel(id))?;
            if theirs.as_deref() != Some(lt.json.as_str()) {
                clashes.push(id.clone());
            }
        } else {
            replay.push(id.clone());
        }
    }

    git::git_reset_hard(&sd, &target_ref)?;

    let id_len = store.load_config()?.id_length;
    let mut used: BTreeSet<String> = target_ids;
    used.extend(local.keys().cloned());
    let mut rename: BTreeMap<String, String> = BTreeMap::new();
    for id in &clashes {
        let nid = fresh_id(&local[id].json, id_len, &used)?;
        used.insert(nid.clone());
        rename.insert(id.clone(), nid);
    }

    for id in replay.iter().chain(clashes.iter()) {
        write_task(&tdir, id, &local[id], &rename)?;
    }
    if !replay.is_empty() || !clashes.is_empty() {
        git::git_add_all(&sd)?;
        git::git_commit(
            &sd,
            &format!(
                "balls: remaster reconcile ({} replayed, {} renamed)",
                replay.len(),
                clashes.len()
            ),
        )?;
    }
    Ok(Reconciled::Joined {
        replayed: replay.len(),
        renamed: clashes.len(),
    })
}

/// Sever shared history: re-root `balls/tasks` as a fresh local
/// orphan carrying its current tasks. The config half (clearing
/// `state_remote`) is the caller's job.
pub fn detach(store: &Store) -> Result<()> {
    git_state::reroot_orphan(
        &store.state_worktree_dir(),
        STATE_BRANCH,
        "balls: remaster --detach (standalone)",
    )
}

fn json_rel(id: &str) -> String {
    format!("{TASKS_REL}/{id}.json")
}

fn read_local_tasks(tdir: &Path) -> Result<BTreeMap<String, LocalTask>> {
    let mut out = BTreeMap::new();
    if !tdir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(tdir)? {
        let path = entry?.path();
        // One fallible chain: non-UTF-8 names and the scaffolding
        // files (`.gitattributes`, `.gitkeep`, `*.notes.jsonl`) all
        // fold into the same skip.
        let Some(id) = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".json"))
            .map(str::to_string)
        else {
            continue;
        };
        let notes_path = tdir.join(format!("{id}.notes.jsonl"));
        out.insert(
            id,
            LocalTask {
                json: fs::read_to_string(&path)?,
                notes: fs::read_to_string(&notes_path).unwrap_or_default(),
            },
        );
    }
    Ok(out)
}

/// A fresh id for a clashing task: regenerate from its own
/// title+timestamp, bumping the timestamp 1ms at a time until it is
/// free across both repos — the same primitive `bl create` uses.
fn fresh_id(json: &str, id_len: usize, used: &BTreeSet<String>) -> Result<String> {
    let t: Task = serde_json::from_str(json)
        .map_err(|e| BallError::InvalidTask(format!("clash task: {e}")))?;
    next_free_id(&t.title, t.created_at, id_len, |id| used.contains(id))
}

/// Pure id-retry loop, mirroring `bl create`'s `next_unique_id`:
/// regenerate from `title`+timestamp, stepping the timestamp forward
/// on collision. Split out so the retry and exhaustion paths are
/// unit-testable without standing up two repos.
fn next_free_id(
    title: &str,
    start: chrono::DateTime<chrono::Utc>,
    id_len: usize,
    taken: impl Fn(&str) -> bool,
) -> Result<String> {
    let mut ts = start;
    for _ in 0..=1000 {
        let id = Task::generate_id(title, ts, id_len);
        if !taken(&id) {
            return Ok(id);
        }
        ts += chrono::Duration::milliseconds(1);
    }
    Err(BallError::Other(
        "could not generate a clash-free id after 1000 tries".into(),
    ))
}

fn write_task(
    tdir: &Path,
    id: &str,
    lt: &LocalTask,
    rename: &BTreeMap<String, String>,
) -> Result<()> {
    let mut v: Value = serde_json::from_str(&lt.json)
        .map_err(|e| BallError::InvalidTask(format!("{id}: {e}")))?;
    let eid = rename.get(id).cloned().unwrap_or_else(|| id.to_string());
    remap_refs(&mut v, &eid, rename);
    let task: Task = serde_json::from_value(v)
        .map_err(|e| BallError::InvalidTask(format!("{id}: {e}")))?;
    task.save(&tdir.join(format!("{eid}.json")))?;
    if !lt.notes.is_empty() {
        fs::write(tdir.join(format!("{eid}.notes.jsonl")), &lt.notes)?;
    }
    Ok(())
}

/// Rewrite this task's own id and any reference (parent, depends_on,
/// links target, archived children) that points at a renamed id.
fn remap_refs(v: &mut Value, eid: &str, rename: &BTreeMap<String, String>) {
    let map = |s: &str| rename.get(s).map_or(s, String::as_str).to_string();
    let Some(o) = v.as_object_mut() else { return };
    o.insert("id".into(), Value::String(eid.to_string()));
    if let Some(Value::String(p)) = o.get("parent") {
        let np = map(p);
        o.insert("parent".into(), Value::String(np));
    }
    remap_array(o.get_mut("depends_on"), &map);
    remap_field_array(o.get_mut("links"), "target", &map);
    remap_field_array(o.get_mut("closed_children"), "id", &map);
}

fn remap_array(v: Option<&mut Value>, map: &impl Fn(&str) -> String) {
    let Some(Value::Array(a)) = v else { return };
    for e in a.iter_mut() {
        if let Value::String(s) = e {
            *s = map(s);
        }
    }
}

fn remap_field_array(v: Option<&mut Value>, key: &str, map: &impl Fn(&str) -> String) {
    let Some(Value::Array(a)) = v else { return };
    for e in a.iter_mut() {
        let Some(o) = e.as_object_mut() else { continue };
        if let Some(Value::String(s)) = o.get(key) {
            let n = map(s);
            o.insert(key.into(), Value::String(n));
        }
    }
}

#[cfg(test)]
#[path = "remaster_tests.rs"]
mod tests;
