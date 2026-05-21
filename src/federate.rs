//! Federation setup — joining a balls project to a hub state-repo
//! (bl-82a4, on top of bl-1098 + bl-ebae). `commands::remaster` calls
//! in on a URL target.
//!
//! The federated sidecars — `.balls/config.json`, `.balls/plugins/`,
//! `.balls/state-repo/` — are gitignored runtime state, recreated by
//! `state_repo::ensure` (the symlinks) on every fresh clone. The only
//! committed federation artifact is the `.balls/master.json` pointer.
//! This module's job at flip time: stash the project-side canonical +
//! plugins aside so `state_repo::ensure` materializes clean symlinks,
//! promote that content up into the hub (hub wins on conflict — no
//! merge), commit the hub canonical onto `balls/tasks`, and write the
//! pointer. `commands::remaster` then does the project-git hygiene
//! (gitignore + untrack). `unfederate` reverses the canonical half.

use crate::config::Config;
use crate::error::Result;
use crate::git_state::STATE_BRANCH;
use crate::master_pointer::MasterPointer;
use crate::state_repo;
use std::fs;
use std::path::{Path, PathBuf};

/// Temp dir a real project-side `.balls/plugins/` is renamed to while
/// `state_repo::ensure` materializes the symlink.
const PLUGINS_STASH: &str = ".balls/plugins.bl82a4-stash";

/// What changed when joining a hub — drives the `bl remaster` summary.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FederateReport {
    pub promoted_plugins: Vec<String>,
    pub discarded_plugins: Vec<String>,
}

/// Wire `root` to the hub at `url`. Materializes the state-repo and
/// both gitignored symlinks, promotes the project canonical + plugins
/// into the hub, and writes `.balls/master.json`. Project-git staging
/// is the caller's job.
pub fn federate(root: &Path, url: &str) -> Result<FederateReport> {
    let mut report = FederateReport::default();

    // Stash project-side canonical + plugins so `state_repo::ensure`'s
    // `ensure_config_symlink` / `ensure_plugins_symlink` see a clean
    // slate and materialize symlinks rather than refusing/no-op'ing.
    let stashed_cfg = stash_config(root)?;
    let stashed_plugins = stash_plugins(root)?;
    state_repo::ensure(root, url)?;

    let state_repo_dir = root.join(state_repo::STATE_REPO_REL);
    let hub_cfg = state_repo_dir.join(".balls").join("config.json");
    let hub_plugins = state_repo_dir.join(".balls").join("plugins");
    fs::create_dir_all(&hub_plugins)?;
    let keep = hub_plugins.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, "")?;
    }

    promote_canonical(stashed_cfg.as_deref(), &hub_cfg, &mut report)?;
    if let Some(stash) = stashed_plugins {
        migrate_stashed_plugins(&stash, &hub_plugins)?;
        fs::remove_dir_all(&stash)?;
    }

    // Commit the hub canonical onto `balls/tasks` so a fresh `git clone`
    // — which carries only `.balls/master.json` — resolves its symlinks
    // after `bl prime`. Best-effort push; `bl sync` carries it otherwise.
    commit_hub_canonical(&state_repo_dir)?;

    MasterPointer {
        master_url: Some(url.to_string()),
        state_remote: None,
    }
    .save(root)?;
    Ok(report)
}

/// `bl remaster <url>` on a fresh git clone with no `.balls/` yet.
/// Seeds a default canonical so the hub gets a real config to adopt.
pub fn bootstrap_non_initted(root: &Path, url: &str) -> Result<FederateReport> {
    fs::create_dir_all(root.join(".balls"))?;
    Config::default().save(&root.join(".balls").join("config.json"))?;
    federate(root, url)
}

/// Read a *real* `.balls/config.json` into memory and remove the file
/// so `state_repo::ensure` materializes a clean symlink. `None` when
/// the canonical is already a symlink or absent.
fn stash_config(root: &Path) -> Result<Option<String>> {
    let p = root.join(".balls").join("config.json");
    if p.is_symlink() || !p.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(&p)?;
    fs::remove_file(&p)?;
    Ok(Some(content))
}

/// Rename a real `.balls/plugins/` aside. `None` when already a
/// symlink or absent.
fn stash_plugins(root: &Path) -> Result<Option<PathBuf>> {
    let project_plugins = root.join(".balls").join("plugins");
    if project_plugins.is_symlink() || !project_plugins.is_dir() {
        return Ok(None);
    }
    let stash = root.join(PLUGINS_STASH);
    if stash.exists() {
        fs::remove_dir_all(&stash)?;
    }
    fs::rename(&project_plugins, &stash)?;
    Ok(Some(stash))
}

/// Move each stashed plugin entry into the hub-side dir unless the
/// hub already owns that name (hub wins).
fn migrate_stashed_plugins(stash: &Path, hub_plugins: &Path) -> Result<()> {
    for entry in fs::read_dir(stash)? {
        let entry = entry?;
        let dest = hub_plugins.join(entry.file_name());
        if dest.exists() {
            continue;
        }
        move_entry(&entry.path(), &dest)?;
    }
    Ok(())
}

/// Commit the hub canonical onto `balls/tasks` in the state-repo,
/// then best-effort push. No-op when the worktree is clean. Stages
/// only the paths that exist — a bootstrap with nothing to promote
/// leaves the hub config absent.
fn commit_hub_canonical(state_repo_dir: &Path) -> Result<()> {
    if !crate::git::has_uncommitted_changes(state_repo_dir)? {
        return Ok(());
    }
    let mut paths: Vec<&Path> = Vec::new();
    if state_repo_dir.join(".balls/config.json").exists() {
        paths.push(Path::new(".balls/config.json"));
    }
    if state_repo_dir.join(".balls/plugins").exists() {
        paths.push(Path::new(".balls/plugins"));
    }
    crate::git::git_add(state_repo_dir, &paths)?;
    crate::git::git_commit(state_repo_dir, "balls: adopt federated project config")?;
    let _ = crate::git::git_push(state_repo_dir, "origin", STATE_BRANCH);
    Ok(())
}

/// Reverse the canonical half of a federation: turn the
/// `.balls/config.json` symlink back into a real file (the hub's
/// last-known content) and drop the `.balls/master.json` pointer. The
/// plugins half is `remaster::detach`'s `restore_plugins_dir`.
/// Idempotent.
pub fn unfederate(root: &Path) -> Result<()> {
    let project_cfg = root.join(".balls").join("config.json");
    if project_cfg.is_symlink() {
        let content = fs::read_to_string(&project_cfg).unwrap_or_default();
        fs::remove_file(&project_cfg)?;
        fs::write(&project_cfg, content)?;
    }
    MasterPointer::default().save(root)?;
    Ok(())
}

/// Promote the stashed project canonical up into the hub config when
/// the hub doesn't already have a meaningful value (hub wins). `None`
/// stashed content ⇒ the hub canonical is already authoritative.
fn promote_canonical(
    stashed: Option<&str>,
    hub: &Path,
    report: &mut FederateReport,
) -> Result<()> {
    let Some(content) = stashed else {
        return Ok(());
    };
    let project_cfg: Config = serde_json::from_str(content)?;
    let mut hub_cfg = if hub.exists() {
        Config::load(hub)?
    } else {
        Config::default()
    };
    merge_canonical(&project_cfg, &mut hub_cfg, report);
    if let Some(parent) = hub.parent() {
        fs::create_dir_all(parent)?;
    }
    hub_cfg.save(hub)?;
    Ok(())
}

/// Merge project-side defaults up into the hub-side config. "Hub
/// wins": only fields the hub left at default (None) absorb the
/// project's value. Bootstrap fields never travel into the canonical.
fn merge_canonical(project: &Config, hub: &mut Config, report: &mut FederateReport) {
    if hub.target_branch.is_none() {
        hub.target_branch.clone_from(&project.target_branch);
    }
    if hub.delivery.is_none() {
        hub.delivery.clone_from(&project.delivery);
    }
    if hub.min_bl_version.is_none() {
        hub.min_bl_version.clone_from(&project.min_bl_version);
    }
    for (name, entry) in &project.plugins {
        if hub.plugins.contains_key(name) {
            report.discarded_plugins.push(name.clone());
        } else {
            hub.plugins.insert(name.clone(), entry.clone());
            report.promoted_plugins.push(name.clone());
        }
    }
    hub.master_url = None;
    hub.state_remote = None;
}

/// Move `src` to `dest`. Both sit under the project root, so a file
/// `rename` always succeeds; directories are copied then removed.
fn move_entry(src: &Path, dest: &Path) -> Result<()> {
    if src.is_dir() {
        copy_dir_recursive(src, dest)?;
        fs::remove_dir_all(src)?;
    } else {
        fs::rename(src, dest)?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Whether the project-side `.balls/` is in the federated shape:
/// `config.json` *and* `plugins/` both symlinks. `bl remaster <url>`
/// short-circuits the idempotent path on this.
pub fn is_federated(root: &Path) -> bool {
    root.join(".balls").join("config.json").is_symlink()
        && root.join(".balls").join("plugins").is_symlink()
}

#[cfg(test)]
#[path = "federate_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "federate_plugins_tests.rs"]
mod plugins_tests;
