//! `bl doctor`: read-only drift diagnostics.
//!
//! Agents and humans get confused when a repo's docs reference bl but
//! the store isn't there, or when it *is* there but drifted. Today
//! those only surface as opaque errors mid-workflow. `doctor` inspects
//! the filesystem and the store, names each problem up front, and
//! points at the command that fixes it. It never mutates state —
//! `repair` stays the only action verb; doctor only ever suggests.

use crate::store::Store;
use crate::task::Status;
use std::fs;
use std::path::Path;

/// One detected drift: what's wrong, and the concrete next step. The
/// discovery-failure passthrough has no `hint` (the message bl already
/// emits is self-contained); structural findings always carry one.
pub struct Finding {
    pub problem: String,
    pub hint: Option<String>,
}

impl Finding {
    fn flag(problem: impl Into<String>, hint: impl Into<String>) -> Self {
        Finding { problem: problem.into(), hint: Some(hint.into()) }
    }
    fn note(problem: impl Into<String>) -> Self {
        Finding { problem: problem.into(), hint: None }
    }
}

/// Doc files whose mention of bl implies the project expects a store.
const DOC_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "README.md", "README"];

/// Substrings that mean "this doc is talking about balls/bl".
const BL_NEEDLES: &[&str] =
    &["bl init", "bl prime", "bl claim", "bl ready", "balls", "bl-"];

/// Run every check from `cwd`. Read-only: opens no write handles,
/// fetches nothing, touches no refs. An empty result means healthy.
pub fn diagnose(cwd: &Path) -> Vec<Finding> {
    match Store::discover(cwd) {
        Ok(store) => check_store(&store),
        Err(e) => check_uninitialized(cwd, &e.to_string()),
    }
}

/// Discovery failed. Surface the (already-actionable, bl-597e) reason,
/// and — only when no `.balls/` exists at all, so the advice is right —
/// connect it to docs that reference bl.
fn check_uninitialized(cwd: &Path, err: &str) -> Vec<Finding> {
    let mut out = vec![Finding::note(format!("bl is not usable here:\n{err}"))];
    if err.contains("no .balls/") && docs_reference_bl(cwd) {
        out.push(Finding::flag(
            "project docs reference bl, but this directory is not bl-initialized",
            "run `bl init` to start tracking here, or remove the bl \
             references from the docs",
        ));
    }
    out
}

fn docs_reference_bl(cwd: &Path) -> bool {
    DOC_FILES.iter().any(|name| {
        fs::read_to_string(cwd.join(name))
            .map(|c| {
                let lc = c.to_lowercase();
                BL_NEEDLES.iter().any(|n| lc.contains(n))
            })
            .unwrap_or(false)
    })
}

fn check_store(store: &Store) -> Vec<Finding> {
    let mut out = Vec::new();
    check_config(store, &mut out);
    check_tasks_dir_override(store, &mut out);
    check_state_worktree(store, &mut out);
    check_stale_claims(store, &mut out);
    check_orphan_worktrees(store, &mut out);
    out
}

fn check_config(store: &Store, out: &mut Vec<Finding>) {
    if let Err(e) = store.load_config() {
        out.push(Finding::flag(
            format!("config at {} is unreadable: {e}", store.config_path().display()),
            "config.json is committed to main — restore it with \
             `git checkout main -- .balls/config.json`",
        ));
    }
}

fn check_tasks_dir_override(store: &Store, out: &mut Vec<Finding>) {
    let f = store.local_dir().join("tasks_dir");
    let Ok(s) = fs::read_to_string(&f) else { return };
    let p = Path::new(s.trim());
    if !p.exists() {
        out.push(Finding::flag(
            format!(
                "tasks_dir override {} points to a missing path: {}",
                f.display(),
                p.display()
            ),
            "fix or remove .balls/local/tasks_dir, or re-run \
             `bl init --tasks-dir <path>`",
        ));
    }
}

fn check_state_worktree(store: &Store, out: &mut Vec<Finding>) {
    if store.stealth {
        return;
    }
    let dir = store.state_worktree_dir();
    if !linked_worktree_ok(&dir) {
        out.push(Finding::flag(
            format!(
                "state worktree at {} is not a valid linked git worktree",
                dir.display()
            ),
            "run `bl repair` to rebuild the state worktree",
        ));
    }
}

/// A linked worktree's `.git` is a *file* whose `gitdir:` target still
/// exists. Checking the pointer directly avoids being fooled by the
/// enclosing repo, which a `git -C` probe would just walk up into.
fn linked_worktree_ok(dir: &Path) -> bool {
    let Ok(content) = fs::read_to_string(dir.join(".git")) else {
        return false;
    };
    let Some(rest) = content.trim().strip_prefix("gitdir:") else {
        return false;
    };
    Path::new(rest.trim()).exists()
}

fn check_stale_claims(store: &Store, out: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(store.claims_dir()) else {
        return;
    };
    for e in entries.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        match store.load_task(&id) {
            Err(_) => out.push(Finding::flag(
                format!("claim file for {id} but no such task in the store"),
                "run `bl repair --fix` to remove orphaned claims",
            )),
            Ok(t) if t.status != Status::InProgress => out.push(Finding::flag(
                format!(
                    "claim file for {id} but its status is {}",
                    t.status.as_str()
                ),
                "run `bl drop` to release it, or `bl repair --fix` to clean up",
            )),
            Ok(_) => {}
        }
    }
}

fn check_orphan_worktrees(store: &Store, out: &mut Vec<Finding>) {
    let Ok(root) = store.worktrees_root() else {
        return;
    };
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for e in entries.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        let claimed = store.claims_dir().join(&id).exists();
        if !claimed && store.load_task(&id).is_err() {
            out.push(Finding::flag(
                format!(
                    "worktree dir {} has no matching claim or task",
                    e.path().display()
                ),
                "run `bl repair --fix` to remove orphaned worktrees",
            ));
        }
    }
}
