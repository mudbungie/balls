//! XDG-first read seam for `Store::discover` per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §7 (Phase 1A).
//!
//! Resolves a clone's on-disk artifacts under the nested XDG layout when
//! they exist. *Read-only*: this module never materializes the XDG tree
//! (fetching the bootstrap branch on first contact is Phase 1B, bound
//! to `bl init`/`bl prime`). A clone whose XDG state is absent falls
//! through to legacy detection in [`crate::store`].
//!
//! Dispatch order matches SPEC §12 row 2 ("Phase 1 (dual-read) reads
//! both layouts and prefers the new one; if the new one is absent,
//! falls back to the old"):
//!
//! 1. `~/.config/balls/<nested>/clone.json` — stealth short-circuit
//!    (SPEC §4.1 / §7 step 2). On `stealth: true`, return a stealth
//!    descriptor pointing at `tasks_dir`; no origin lookup, no
//!    trackers/ probe.
//! 2. `~/.local/state/balls/trackers/<enc-origin>/<enc-balls-tasks>/`
//!    — the bootstrap-branch checkout (SPEC §5 step 2). If the
//!    checkout's `.git` exists, the clone is in XDG mode; build the
//!    per-clone tree paths and return.
//! 3. None — caller falls back to legacy.
//!
//! Phase 1A scope: read existing XDG state, no fetch. Phase 1B (the
//! `bl init` flip) creates the XDG tree on a fresh clone; Phase 1C
//! moves worktrees there on claim. Materialization on first contact
//! (`bl prime` rebuilds after `rm -rf ~/.local/state/balls/trackers/`)
//! is part of Phase 1B's prime path — Phase 1A only finds.
//!
//! The redirect follow (SPEC §5 step 3) is *not* implemented here yet
//! — the §14.5 chained-redirect gate is delegated to a follow-up slice
//! within Phase 1A once `tracker.json::read_optional` is wired into
//! discover (currently the foundation module is loaded but unread by
//! `Store::discover`). The single-hop solo path covers the §14.7 gate.

use crate::clone_json::CloneJson;
use crate::encoding::{canonicalize_origin, nested_clone_path, percent_encode_component};
use crate::error::{BallError, Result};
use crate::repo_json::RepoJson;
use crate::store::{Layout, Store};
use crate::tracker_address;
use crate::tracker_json::TrackerJson;
use crate::xdg_paths::{clone_json_path, own_tracker_checkout, tracker_checkout, PerClonePaths, XdgBases};
use std::path::{Path, PathBuf};

/// One XDG-mode discovery outcome. Carries every path
/// `Store::discover` needs to populate its layout-keyed fields.
///
/// `stealth` collapses the trackers/per-clone bundle: a stealth
/// clone has no tracker checkout (origin-less by design) and only the
/// per-clone catch-all tree under `~/.local/state/balls/`. Non-stealth
/// carries both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgResolution {
    pub stealth: bool,
    /// `<nested-clone-path>` — the clone's absolute path with the
    /// leading `/` dropped (§4). Saved for the per-clone tree builder.
    pub nested_clone: PathBuf,
    /// Where task files actually live. Tracker `.balls/tasks/` (own
    /// or federated) in normal mode; `clone.tasks_dir` in stealth.
    pub tasks_dir: PathBuf,
    /// The git directory hosting the `balls/tasks` branch. The tracker
    /// checkout in normal mode; a meaningless sentinel in stealth (the
    /// per-clone catch-all dir, by convention).
    pub state_repo: PathBuf,
    /// Per-clone working dirs — `worktrees/`, `claims/`, `locks/`,
    /// `plugins-auth/` under `<nested-clone-path>`.
    pub per_clone: PerClonePaths,
    /// `<state-root>/state/<nested-clone-path>` — the per-clone
    /// catch-all for runtime state SPEC §3 does not name explicitly
    /// (pending-sync queue, squash scratch, the legacy LocalConfig
    /// path). Not user-visible.
    pub local_dir: PathBuf,
    /// `<tracker>/.balls/repo.json` (normal mode) or absent (stealth).
    /// Stealth clones have no `repo.json` (no tracker branch).
    pub repo_json_path: Option<PathBuf>,
    /// `<tracker>/.balls/project.json` (normal mode) or absent
    /// (stealth — there is no project-wide config to inherit).
    pub project_config_path: Option<PathBuf>,
    /// The clone.json file, if any.
    pub clone_json_file: PathBuf,
    /// Parsed clone.json contents (None when the file is absent).
    pub clone_json: Option<CloneJson>,
}

/// Try to resolve `clone_root` as an XDG-layout clone. Returns
/// `Ok(None)` when no XDG state is found — the caller (Store) falls
/// back to legacy detection. Returns `Ok(Some(_))` on a stealth
/// clone.json or a materialized tracker checkout. Errors are reserved
/// for malformed but-present XDG state (a corrupt clone.json, a
/// tracker checkout the user can't `stat`).
///
/// `clone_root` is the clone's working tree (or bare-clone parent).
/// `origin_url` is the result of `git remote get-url origin` — `None`
/// when no origin is set. The split lets `Store::discover` shell out
/// to git once and pass the result in; this function does no git ops.
pub fn try_resolve(
    bases: &XdgBases,
    clone_root: &Path,
    origin_url: Option<&str>,
) -> Result<Option<XdgResolution>> {
    let nested = nested_clone_path(clone_root);
    let clone_json_file = clone_json_path(bases, &nested);
    let clone_json = CloneJson::read_optional(&clone_json_file)?;

    if let Some(cj) = clone_json.as_ref() {
        if cj.stealth {
            return Ok(Some(build_stealth(bases, &nested, &clone_json_file, cj)));
        }
    }

    let Some(url) = origin_url else {
        // No origin and no stealth clone.json — XDG cannot resolve
        // (§4: non-stealth requires origin). Caller falls back.
        return Ok(None);
    };
    let enc_origin = percent_encode_component(&canonicalize_origin(url));
    let own_checkout = own_tracker_checkout(bases, &enc_origin);
    if !own_checkout.join(".git").exists() {
        // XDG layout not yet materialized for this clone.
        return Ok(None);
    }

    let active_checkout = resolve_redirect(bases, &own_checkout)?;
    let per_clone = PerClonePaths::new(bases, &nested);
    let local_dir = bases.state_root().join("state").join(&nested);
    Ok(Some(XdgResolution {
        stealth: false,
        nested_clone: nested,
        tasks_dir: active_checkout.join(".balls/tasks"),
        state_repo: active_checkout.clone(),
        per_clone,
        local_dir,
        repo_json_path: Some(own_checkout.join(".balls/repo.json")),
        project_config_path: Some(active_checkout.join(".balls/project.json")),
        clone_json_file,
        clone_json,
    }))
}

/// Follow the SPEC §5 single-hop redirect. Returns the *active*
/// tracker checkout — own when there is no redirect, the federated
/// checkout when `tracker.json` points to one. Aborts on a chained
/// redirect (§14.5 — defense-in-depth even though §5 forbids the
/// federated branch from carrying its own `tracker.json`).
fn resolve_redirect(bases: &XdgBases, own_checkout: &Path) -> Result<PathBuf> {
    let tj_path = own_checkout.join(".balls/tracker.json");
    let Some(tj) = TrackerJson::read_optional(&tj_path)? else {
        return Ok(own_checkout.to_path_buf());
    };
    let federated = tracker_checkout(
        bases,
        &percent_encode_component(&canonicalize_origin(&tj.state_url)),
        &percent_encode_component(tj.effective_branch()),
    );
    // SPEC §5: federated tracker MUST NOT itself carry tracker.json.
    let chained = federated.join(".balls/tracker.json");
    if chained.exists() {
        return Err(BallError::Other(format!(
            "chained redirect detected: {} → {} → ... \
             (SPEC-clone-layout §5: single-hop only)",
            own_checkout.display(),
            federated.display()
        )));
    }
    Ok(federated)
}

/// Build the stealth-mode resolution. SPEC §4.1: read clone.json's
/// `tasks_dir` and skip every origin/trackers/redirect step. The
/// `repo_json_path`/`project_config_path` are absent — stealth clones
/// have no tracker branch.
///
/// `cj.tasks_dir` is `Some` by construction: `CloneJson::from_json`
/// runs `validate_stealth` which already rejects `stealth=true` with
/// no `tasks_dir`, so reaching this function with a `None` tasks_dir
/// is unreachable.
fn build_stealth(
    bases: &XdgBases,
    nested: &Path,
    clone_json_file: &Path,
    cj: &CloneJson,
) -> XdgResolution {
    let tasks_dir = PathBuf::from(
        cj.tasks_dir
            .as_deref()
            .expect("stealth clone.json must carry tasks_dir; validate_stealth gates the read"),
    );
    let per_clone = PerClonePaths::new(bases, nested);
    let local_dir = bases.state_root().join("state").join(nested);
    XdgResolution {
        stealth: true,
        nested_clone: nested.to_path_buf(),
        tasks_dir,
        // Stealth has no state branch; point at the catch-all so callers
        // that compute paths under it don't panic on a sentinel path.
        state_repo: local_dir.clone(),
        per_clone,
        local_dir,
        repo_json_path: None,
        project_config_path: None,
        clone_json_file: clone_json_file.to_path_buf(),
        clone_json: Some(cj.clone()),
    }
}

/// Read the resolution's `repo.json` if a path is present, otherwise
/// return defaults. Convenience for `Store::discover` — keeps the
/// "stealth has no repo.json" branch out of the discover seam.
pub fn read_repo_json(res: &XdgResolution) -> Result<RepoJson> {
    match res.repo_json_path.as_ref() {
        Some(p) => RepoJson::read_or_default(p),
        None => Ok(RepoJson::default()),
    }
}

/// Try to open `clone_root` as an XDG-mode Store. Returns
/// `Ok(None)` to fall through to legacy when no XDG state is
/// materialized. Wraps [`try_resolve`] with the env read + the
/// Store-building step so `Store::discover_git` stays trim.
pub fn try_open(clone_root: &Path) -> Result<Option<Store>> {
    let Some(bases) = XdgBases::from_env() else { return Ok(None); };
    let origin_url = crate::repo_url::origin_url(clone_root);
    let Some(res) = try_resolve(&bases, clone_root, origin_url.as_deref())? else {
        return Ok(None);
    };
    Ok(Some(build_store(clone_root.to_path_buf(), res)))
}

fn build_store(root: PathBuf, res: XdgResolution) -> Store {
    let state_branch = if res.stealth {
        tracker_address::DEFAULT_BRANCH.to_string()
    } else {
        crate::git::git_current_branch(&res.state_repo)
            .unwrap_or_else(|_| tracker_address::DEFAULT_BRANCH.to_string())
    };
    // XDG has no `.balls/config.json` analog at the clone root.
    // For Phase 1A, point `config_path()` at the resolved `repo.json`
    // when present (the new home for the same fields), falling back
    // to `clone.json` so callers that read overrides still find a file.
    let config_file_path = res
        .repo_json_path
        .clone()
        .unwrap_or_else(|| res.clone_json_file.clone());
    let project_config_file_path = res
        .project_config_path
        .clone()
        .unwrap_or_else(|| res.clone_json_file.clone());
    Store {
        root,
        stealth: res.stealth,
        no_git: false,
        layout: Layout::Xdg,
        tasks_dir_path: res.tasks_dir,
        state_repo_path: res.state_repo,
        state_branch_name: state_branch,
        claims_dir_path: res.per_clone.claims,
        lock_dir_path: res.per_clone.locks,
        local_plugins_dir_path: res.per_clone.plugins_auth,
        worktrees_root_path: res.per_clone.worktrees,
        local_dir_path: res.local_dir,
        config_file_path,
        project_config_file_path,
    }
}

#[cfg(test)]
#[path = "xdg_discover_tests.rs"]
mod tests;
