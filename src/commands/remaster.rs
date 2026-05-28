//! `bl remaster` — write or remove the `.balls/tracker.json` redirect
//! on the code repo's own `balls/tasks` branch checkout (SPEC §6.1).
//!
//! Pre-XDG, remaster wrote `state_url` / `state_branch` to
//! `.balls/config.json` on `main`. Post-XDG (Phase 1B-7), the redirect
//! is a pointer-only `tracker.json` carried on the own `balls/tasks`
//! branch checkout under `~/.local/state/balls/trackers/<enc-origin>/
//! balls%2Ftasks/`. The federated tracker checkout under
//! `trackers/<enc-state-url>/<enc-state-branch>/` is materialized by
//! `Store::discover` on next run; this command only ever writes (or
//! removes) the pointer file.

use super::discover;
use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::error::{BallError, Result};
use balls::repo_url;
use balls::state_repo;
use balls::store::{Layout, Store};
use balls::tracker_json::TrackerJson;
use balls::xdg_paths::{own_tracker_checkout, XdgBases};
use balls::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

const TRACKER_REL: &str = ".balls/tracker.json";

pub fn cmd_remaster(
    target: Option<String>,
    branch: Option<String>,
    commit: bool,
    detach: bool,
) -> Result<()> {
    if detach && target.is_some() {
        return Err(BallError::Other(
            "remaster --detach takes no TARGET (it goes standalone)".into(),
        ));
    }
    let store = discover()?;
    let own = require_xdg_own_checkout(&store)?;
    let tj_path = own.join(TRACKER_REL);
    if detach {
        return detach_redirect(&own, &tj_path, commit);
    }
    let target = target.ok_or_else(|| {
        BallError::Other("remaster needs a TARGET tracker URL (or use --detach)".into())
    })?;
    let url = resolve_target(&store.root, &target);
    write_redirect(&own, &tj_path, &url, branch.as_deref(), commit)
}

/// Resolve the XDG own-checkout for this clone — `trackers/<enc-origin>/
/// balls%2Ftasks/`. HOME, origin, and the own checkout's existence are
/// guaranteed by `Store::discover` returning `Layout::Xdg` + non-stealth
/// (the `xdg_discover` seam refuses to resolve XDG mode without all
/// three), so the only invariants we recheck are the two user-facing
/// ones: layout and stealth.
fn require_xdg_own_checkout(store: &Store) -> Result<PathBuf> {
    if store.layout != Layout::Xdg {
        return Err(BallError::Other(
            "bl remaster requires the XDG layout; run `bl migrate` first".into(),
        ));
    }
    if store.stealth {
        return Err(BallError::Other(
            "bl remaster cannot operate on a stealth clone".into(),
        ));
    }
    let bases = XdgBases::from_env().expect("HOME set (xdg_discover guaranteed)");
    let url = repo_url::origin_url(&store.root).expect("origin set (xdg_discover guaranteed)");
    let enc_origin = percent_encode_component(&canonicalize_origin(&url));
    Ok(own_tracker_checkout(&bases, &enc_origin))
}

/// Write `tracker.json` on the own checkout. `--commit` stages and
/// commits the file on `balls/tasks`; otherwise the change is left
/// uncommitted so the user can inspect before publishing.
fn write_redirect(
    own: &Path,
    tj_path: &Path,
    url: &str,
    branch: Option<&str>,
    commit: bool,
) -> Result<()> {
    let tj = TrackerJson {
        state_url: url.to_string(),
        state_branch: branch.map(String::from),
    };
    fs::create_dir_all(tj_path.parent().expect(".balls/tracker.json has parent"))?;
    fs::write(tj_path, tj.to_json()? + "\n")?;
    if commit {
        git::git_add(own, &[Path::new(TRACKER_REL)])?;
        git::git_commit(own, "balls: remaster — set tracker address")?;
    }
    let state = if commit { "committed" } else { "wrote (uncommitted)" };
    println!("{state} {TRACKER_REL} → {url}");
    Ok(())
}

/// Remove `tracker.json` from the own checkout. `--commit` records
/// the removal on `balls/tasks`. A missing file is a no-op.
fn detach_redirect(own: &Path, tj_path: &Path, commit: bool) -> Result<()> {
    if !tj_path.exists() {
        println!("already detached: no {TRACKER_REL} on balls/tasks");
        return Ok(());
    }
    fs::remove_file(tj_path)?;
    if commit {
        git::git_add(own, &[Path::new(TRACKER_REL)])?;
        git::git_commit(own, "balls: remaster --detach (standalone)")?;
    }
    let state = if commit { "committed" } else { "removed (uncommitted)" };
    println!("{state} detach: cleared {TRACKER_REL}");
    Ok(())
}

/// A bare git-remote name on the *code* repo is resolved to its URL;
/// a URL or path is passed through. `tracker.json` always carries a
/// URL, never a remote shortname.
fn resolve_target(root: &Path, target: &str) -> String {
    if state_repo::looks_like_url(target) {
        return target.to_string();
    }
    git_state::remote_url(root, target).unwrap_or_else(|| target.to_string())
}
