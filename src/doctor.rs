//! `bl doctor`: read-only drift diagnostics.
//!
//! Agents and humans get confused when a repo's docs reference bl but
//! the store isn't there, or when it *is* there but drifted. Today
//! those only surface as opaque errors mid-workflow. `doctor` inspects
//! the filesystem and the store, names each problem up front, and
//! points at the command that fixes it. It never mutates state —
//! `repair` stays the only action verb; doctor only ever suggests.

use crate::doctor_symlink::check_tasks_symlink;
use crate::store::{Layout, Store};
use crate::task::Status;
use crate::xdg_paths::XdgBases;
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
    pub fn flag(problem: impl Into<String>, hint: impl Into<String>) -> Self {
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
        fs::read_to_string(cwd.join(name)).is_ok_and(|c| {
            let lc = c.to_lowercase();
            BL_NEEDLES.iter().any(|n| lc.contains(n))
        })
    })
}

fn check_store(store: &Store) -> Vec<Finding> {
    let mut out = Vec::new();
    crate::doctor_config::check_config(store, &mut out);
    check_tasks_dir_override(store, &mut out);
    check_local_legacy_markers(store, &mut out);
    check_state_repo(store, &mut out);
    check_stale_claims(store, &mut out);
    check_orphan_worktrees(store, &mut out);
    check_legacy_layout(store, &mut out);
    check_legacy_pending_sync(store, &mut out);
    check_moved_clone(store, &mut out);
    out
}

/// bl-341b: surface the pre-bl-6969 `<repo>/.balls/local/pending-sync/`
/// tree if a clone still carries one. Implementation lives in the
/// sibling `doctor_pending_sync` module for the file-size budget;
/// doctor is read-only, so we only suggest a manual cleanup.
fn check_legacy_pending_sync(store: &Store, out: &mut Vec<Finding>) {
    if let Some(f) = crate::doctor_pending_sync::finding(&store.root) {
        out.push(f);
    }
}

/// Phase 3 (bl-05e5) / SPEC-clone-layout §12: when the resolved store
/// is the legacy layout, list the markers and name `bl prime --migrate`
/// or `bl migrate` as the fix. Read-only — never converts.
fn check_legacy_layout(store: &Store, out: &mut Vec<Finding>) {
    if store.layout != Layout::Legacy {
        return;
    }
    if let Some(finding) = legacy_layout_finding(&store.root) {
        out.push(finding);
    }
}

/// Pure helper: build the legacy-layout finding for `root`, or
/// `None` when no markers are present. Pulled out so the empty-arm
/// has a direct unit test (a Layout::Legacy store with no markers
/// is unreachable from `bl init`, but the gate stays defensive).
pub(crate) fn legacy_layout_finding(root: &Path) -> Option<Finding> {
    let markers = crate::legacy_layout::detect(root);
    if markers.is_empty() {
        return None;
    }
    let paths: Vec<String> = markers.iter().map(|m| m.path.display().to_string()).collect();
    Some(Finding::flag(
        format!("legacy layout in use; markers: {}", paths.join(", ")),
        "run `bl prime --migrate` (or `bl migrate`) to relocate this clone \
         onto the XDG layout (SPEC-clone-layout §11.1)",
    ))
}

/// Phase 3 (bl-05e5) / SPEC §8 + §14.14: surface orphaned per-clone
/// state from a moved clone. Each orphan becomes one finding naming
/// the old subtree, the orphan task IDs, and the `bl repair
/// --rebind-path` command. Stealth/legacy clones have no XDG
/// per-clone tree to walk; skip them.
fn check_moved_clone(store: &Store, out: &mut Vec<Finding>) {
    if store.stealth || store.layout != Layout::Xdg {
        return;
    }
    let Some(bases) = XdgBases::from_env() else { return };
    let orphans = crate::doctor_moved::find_orphans(&bases, &store.root);
    out.extend(crate::doctor_moved::to_findings(&orphans, &store.root));
}

/// SPEC §6.4: stealth clones name their task store via
/// `clone.json.tasks_dir`. If the path is set but missing on disk the
/// store is unreachable and bl will fail every read; surface it as a
/// finding so `bl doctor` names the file instead of letting the next
/// command emit an opaque ENOENT.
fn check_tasks_dir_override(store: &Store, out: &mut Vec<Finding>) {
    let Some(cj) = store.clone_json() else { return };
    let Some(td) = cj.tasks_dir.as_deref() else { return };
    let p = Path::new(td);
    if !p.exists() {
        out.push(Finding::flag(
            format!(
                "clone.json tasks_dir points to a missing path: {}",
                p.display()
            ),
            "fix the path on disk, or update clone.json (\
             `~/.config/balls/<nested-clone-path>/clone.json`) — \
             SPEC-clone-layout §6.4",
        ));
    }
}

/// bl-5a03: `.balls/local/config.json` and `.balls/local/tasks_dir`
/// are the pre-XDG per-clone override files. The reader retired with
/// this ball; legacy clones that still carry the files quietly lose
/// their override surface until the user converts. Surface each one
/// as a finding so the loss isn't silent. No auto-import — clone.json
/// fields differ in name/shape and the user is in the best position
/// to translate the intent (e.g. drop the override, or map plugin
/// participant policy back to `project.json`).
fn check_local_legacy_markers(store: &Store, out: &mut Vec<Finding>) {
    let local = store.root.join(".balls/local");
    let cfg = local.join("config.json");
    if cfg.exists() {
        out.push(Finding::flag(
            format!(
                "{} is a pre-XDG per-clone override; bl no longer reads it",
                cfg.display()
            ),
            "translate the `require_remote_on_*` fields into \
             `~/.config/balls/<nested-clone-path>/clone.json` \
             (SPEC-clone-layout §6.4), then delete the legacy file",
        ));
    }
    let td = local.join("tasks_dir");
    if td.exists() {
        out.push(Finding::flag(
            format!(
                "{} is a pre-XDG stealth marker; bl no longer reads it",
                td.display()
            ),
            "move the path into `clone.json.tasks_dir` (with \
             `stealth: true`) under `~/.config/balls/<nested-clone-path>/`, \
             then delete the legacy file (SPEC-clone-layout §6.4)",
        ));
    }
}

/// Validate the unified state checkout (SPEC-tracker-state §4):
/// `.balls/state-repo` is a full git clone — a `.git` *directory*
/// with a HEAD — and the `.balls/tasks` convenience symlink resolves
/// into it. Legacy-only: XDG clones host the tracker checkout under
/// `~/.local/state/balls/trackers/...` and have no `.balls/` at the
/// clone root. Stealth repos have no state checkout, nothing to check.
fn check_state_repo(store: &Store, out: &mut Vec<Finding>) {
    if store.stealth || store.layout == Layout::Xdg {
        return;
    }
    let dir = store.state_repo_dir();
    if !state_repo_ok(&dir) {
        out.push(Finding::flag(
            format!("state checkout at {} is not a valid git clone", dir.display()),
            format!(
                "remove {} and re-run `bl prime` to re-materialize it from \
                 the tracker address",
                dir.display()
            ),
        ));
        return;
    }
    check_tasks_symlink(&store.root, "state-repo/.balls/tasks", out);
}

/// The state checkout is a full clone; `.git` must be a directory
/// with a HEAD (a `.git` file would be a stray linked-worktree pointer).
fn state_repo_ok(dir: &Path) -> bool {
    dir.join(".git").is_dir() && dir.join(".git/HEAD").exists()
}

fn check_stale_claims(store: &Store, out: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(store.claims_dir()) else {
        return;
    };
    for e in entries.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        // The Phase 3 (bl-05e5) moved-clone breadcrumb sits in
        // `claims/<nested>/clone-path.json`. It is not a claim;
        // filter it out so it doesn't surface as a phantom orphan.
        if !id.starts_with("bl-") {
            continue;
        }
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

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod legacy_finding_tests;
