//! XDG-only `bl init` per [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md)
//! §3, §5, §6, §11 (Phase 1B, bl-e802).
//!
//! Two shapes:
//!
//! - **Non-stealth.** Requires `origin`. Materializes the tracker
//!   checkout at `~/.local/state/balls/trackers/<enc-origin>/balls%2Ftasks/`,
//!   seeds `.balls/{tasks,repo.json,project.json}` on the bootstrap
//!   branch (no `tracker.json` — solo case, SPEC §5 case 2), commits,
//!   and best-effort pushes. No commit on `main`; no `.gitignore`
//!   touched.
//! - **Stealth.** Writes `~/.config/balls/<nested>/clone.json` with
//!   `stealth: true` + `tasks_dir`. No tracker checkout, no origin
//!   required. SPEC §4.1.
//!
//! After writing, the function returns via `Store::discover`, which
//! re-reads the layout through `xdg_discover` — single source of
//! truth for what the running binary sees.
//!
//! Tests live in `tests/conformance_xdg_init.rs` (§14.1, §14.4,
//! §14.15, §14.19) and exercise the binary end-to-end.

use crate::clone_json::CloneJson;
use crate::encoding::{canonicalize_origin, nested_clone_path, percent_encode_component};
use crate::error::{BallError, Result};
use crate::project_config::ProjectConfig;
use crate::repo_json::RepoJson;
use crate::store::Store;
use crate::xdg_paths::{clone_json_path, own_tracker_checkout, PerClonePaths, XdgBases};
use crate::{git, git_state, repo_url};
use std::fs;
use std::path::{Path, PathBuf};

/// SPEC §5: the orphan branch name. Compiled in; not configurable.
const BOOTSTRAP_BRANCH: &str = "balls/tasks";

/// Entry point for the XDG `bl init`. Dispatches stealth vs origin
/// and returns the discovered Store on success.
pub fn init(from: &Path, stealth: bool, tasks_dir: Option<String>) -> Result<Store> {
    if let Some(ref td) = tasks_dir {
        if !Path::new(td).is_absolute() {
            return Err(BallError::Other(format!(
                "--tasks-dir must be an absolute path, got: {td}"
            )));
        }
    }
    let bases = XdgBases::from_env().ok_or_else(|| {
        BallError::Other("HOME must be set for XDG bl init".into())
    })?;
    let use_stealth = stealth || tasks_dir.is_some();
    let discover_from = if use_stealth {
        init_stealth(&bases, from, tasks_dir)?
    } else {
        init_with_origin(&bases, from)?
    };
    Store::discover(&discover_from)
}

/// SPEC §4.1: write `clone.json` carrying `stealth: true` + `tasks_dir`.
/// No origin lookup, no tracker checkout, no `.balls/` at the clone
/// root. Returns the identity path the caller should `discover` from
/// — when `--tasks-dir` was given that path *is* the identity, not
/// `cwd`.
fn init_stealth(bases: &XdgBases, from: &Path, tasks_dir: Option<String>) -> Result<PathBuf> {
    let cwd = fs::canonicalize(from).unwrap_or_else(|_| from.to_path_buf());
    let td = tasks_dir
        .as_deref()
        .map_or_else(|| cwd.join(".balls/tasks"), PathBuf::from);
    fs::create_dir_all(&td)?;
    // SPEC §4.1: stealth clone's <nested-clone-path> is the tasks_dir
    // if supplied, otherwise the cwd at bl init --stealth time.
    let identity_path: PathBuf = if tasks_dir.is_some() { td.clone() } else { cwd };
    let nested = nested_clone_path(&identity_path);
    let cj = CloneJson {
        stealth: true,
        tasks_dir: Some(td.to_string_lossy().into_owned()),
        ..Default::default()
    };
    cj.save(&clone_json_path(bases, &nested))?;
    ensure_per_clone(bases, &nested)?;
    Ok(identity_path)
}

/// Non-stealth init. Requires `origin` so the clone has a tracker
/// address. Idempotent: a warm tracker checkout is reused.
fn init_with_origin(bases: &XdgBases, from: &Path) -> Result<PathBuf> {
    let repo_root = git::git_root(from)?;
    git::git_ensure_user(&repo_root)?;
    // SPEC §14.19: no balls-attributed commits on main. Pre-XDG init
    // seeded an "initial commit" on main so the rest of the legacy
    // pipeline had a HEAD to work against; XDG init writes nothing on
    // the clone branch, so the seed is gone — `bl create` and friends
    // operate on the tracker checkout, not on main.
    let url = repo_url::origin_url(&repo_root).ok_or_else(|| {
        BallError::Other(
            "no origin configured — set `git remote add origin <url>` or use --stealth".into(),
        )
    })?;
    let enc_origin = percent_encode_component(&canonicalize_origin(&url));
    let tracker = own_tracker_checkout(bases, &enc_origin);
    materialize_tracker(&tracker, &url)?;
    seed_tracker_branch(&tracker)?;
    let nested = nested_clone_path(&repo_root);
    ensure_per_clone(bases, &nested)?;
    Ok(repo_root)
}

/// Per-clone XDG dirs (claims/, locks/, plugins-auth/ under
/// `<nested>`). worktrees/ is materialized lazily by `bl claim`
/// (Phase 1C).
fn ensure_per_clone(bases: &XdgBases, nested: &Path) -> Result<()> {
    let per = PerClonePaths::new(bases, nested);
    for d in [&per.claims, &per.locks, &per.plugins_auth] {
        fs::create_dir_all(d)?;
    }
    Ok(())
}

/// Create or warm the tracker checkout at `dir`. Online: track
/// `origin/balls/tasks` when it exists, else create + push the
/// orphan. Offline first-contact: create local orphan so the rest of
/// `bl init` can seed; the next `bl sync` will publish. Idempotent.
fn materialize_tracker(dir: &Path, url: &str) -> Result<()> {
    if dir.join(".git").exists() {
        return warm_tracker(dir, url);
    }
    fs::create_dir_all(dir)?;
    let parent = dir.parent().expect("trackers/<enc> has parent");
    fs::create_dir_all(parent)?;
    git::run_git_ok(
        parent,
        &[
            "init",
            "-q",
            "--initial-branch",
            BOOTSTRAP_BRANCH,
            &dir.to_string_lossy(),
        ],
    )?;
    git::run_git_ok(dir, &["remote", "add", "origin", url])?;
    git::git_ensure_user(dir)?;
    let _ = git::git_fetch(dir, "origin");
    if git_state::has_remote_branch(dir, "origin", BOOTSTRAP_BRANCH) {
        git_state::create_tracking_branch(dir, BOOTSTRAP_BRANCH, "origin")?;
    } else {
        git_state::create_orphan_branch(dir, BOOTSTRAP_BRANCH, "balls: init")?;
    }
    git::run_git_ok(dir, &["checkout", "-q", BOOTSTRAP_BRANCH])?;
    Ok(())
}

/// Warm path: align the remote origin URL to `url`. The cold path
/// already left HEAD on `BOOTSTRAP_BRANCH` and the `origin` remote
/// configured, so warm-up just re-applies the URL in case the user
/// rotated origin.
fn warm_tracker(dir: &Path, url: &str) -> Result<()> {
    git::git_ensure_user(dir)?;
    git::git_config_set(dir, "remote.origin.url", url)?;
    Ok(())
}

/// Seed the bootstrap branch with `.balls/tasks/`, `.balls/repo.json`,
/// `.balls/project.json`. Commits once and best-effort pushes.
fn seed_tracker_branch(dir: &Path) -> Result<()> {
    let balls = dir.join(".balls");
    let tasks = balls.join("tasks");
    fs::create_dir_all(&tasks)?;
    let attrs = tasks.join(".gitattributes");
    let need_attrs = fs::read_to_string(&attrs)
        .map_or(true, |s| !s.contains("*.notes.jsonl merge=union"));
    if need_attrs {
        fs::write(&attrs, "*.notes.jsonl merge=union\n")?;
    }
    let keep = tasks.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }
    let repo_json = balls.join("repo.json");
    if !repo_json.exists() {
        fs::write(&repo_json, RepoJson::default().to_json()? + "\n")?;
    }
    let project_json = balls.join("project.json");
    if !project_json.exists() {
        ProjectConfig::default().save(&project_json)?;
    }
    if git::has_uncommitted_changes(dir)? {
        git::git_add_all(dir)?;
        git::git_commit(dir, "balls: seed state branch")?;
    }
    let _ = git::git_push(dir, "origin", BOOTSTRAP_BRANCH);
    Ok(())
}

// End-to-end coverage lives in `tests/conformance_xdg_init.rs`
// (§14.1, §14.4, §14.15, §14.19) and the rewritten `tests/init*.rs`
// suite. The seams here (origin resolution, stealth dispatch, branch
// creation) all need a real HOME, git, and origin to behave; a
// per-fn unit mock would catch none of the integration concerns
// the module exists to handle.
