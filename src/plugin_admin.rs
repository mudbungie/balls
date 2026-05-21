//! `bl plugin enable/disable/list` — first-class plugin management
//! (bl-32e5). The `plugins` map is project config (SPEC §7): it lives
//! in `.balls/project.json` on the tracker branch, reached through the
//! `.balls/project.json` symlink, alongside the per-plugin config
//! files under `.balls/plugins/`. `enable`/`disable` update the map in
//! place and `commit_change` commits both `project.json` and the
//! touched config files onto the tracker branch in one step.

use crate::error::{BallError, NotInitKind, Result};
use crate::project_config::{PluginEntry, ProjectConfig};
use crate::git;
use crate::store::Store;
use crate::store_lock;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the effective plugins map lives — `.balls/project.json`, the
/// project config on the tracker branch (SPEC §7). The single seam the
/// admin surface and `load_effective` route through.
pub fn effective_config_path(store: &Store) -> PathBuf {
    store.project_config_path()
}

pub fn effective_plugins_dir(store: &Store) -> PathBuf {
    store.plugin_config_root().join(".balls/plugins")
}

/// Validate the plugin name once at the command boundary. The runtime
/// trusts loaded config; this is the gate that keeps a typo from
/// committing `../foo` or whitespace into the plugins map.
pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(BallError::Other("plugin name must not be empty".into()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(BallError::Other(format!(
            "invalid plugin name {name:?}: use ASCII letters/digits/`-`/`_`"
        )));
    }
    Ok(())
}

/// Reject `config_file` values that would escape the plugins root.
/// Mirrors `Config::validate`'s worktree_dir rule.
fn validate_config_file(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(BallError::Other("--config-file must not be empty".into()));
    }
    if value.starts_with('/') || value.contains("..") {
        return Err(BallError::Other(format!(
            "--config-file {value:?}: must be a relative path with no '..' segments"
        )));
    }
    Ok(())
}

/// The effective plugins map. Single read used by both `list` and the
/// enable/disable mutators to surface drift errors at the same moment.
pub fn load_effective(store: &Store) -> Result<BTreeMap<String, PluginEntry>> {
    Ok(load_or_default(&effective_config_path(store))?.plugins)
}

/// Add or replace the entry for `name`. Re-running with different
/// flags overwrites — making `enable` idempotent in the
/// observable-state sense, not field-preserving. The `participant`
/// block (set elsewhere) is preserved so this surface never silently
/// drops policy a maintainer set by hand.
pub fn enable(
    store: &Store,
    name: &str,
    config_file: Option<String>,
    sync_on_change: bool,
) -> Result<EnableReport> {
    validate_name(name)?;
    let cfg_path = effective_config_path(store);
    ensure_parent(&cfg_path)?;
    let resolved_file = config_file.unwrap_or_else(|| format!("{name}.json"));
    validate_config_file(&resolved_file)?;

    let mut cfg = load_or_default(&cfg_path)?;
    let participant = cfg.plugins.get(name).and_then(|e| e.participant.clone());
    cfg.plugins.insert(
        name.to_string(),
        PluginEntry {
            enabled: true,
            sync_on_change,
            config_file: resolved_file.clone(),
            participant,
        },
    );
    cfg.validate()?;

    let plugins_dir = effective_plugins_dir(store);
    fs::create_dir_all(&plugins_dir)?;
    let file_path = plugins_dir.join(&resolved_file);
    let file_created = !file_path.exists();
    if file_created {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file_path, "{}\n")?;
    }
    cfg.save(&cfg_path)?;

    commit_change(store, &format!("balls: plugin enable {name}"))?;
    Ok(EnableReport { config_path: cfg_path, file_path, file_created })
}

/// Remove the entry. The per-plugin config file is kept on disk —
/// operators may want to disable a plugin temporarily without
/// re-entering credentials. Errors if `name` isn't in the map; silent
/// disable would be ambiguous with a typo.
pub fn disable(store: &Store, name: &str) -> Result<DisableReport> {
    validate_name(name)?;
    let cfg_path = effective_config_path(store);
    let mut cfg = load_or_default(&cfg_path)?;
    if cfg.plugins.remove(name).is_none() {
        return Err(BallError::Other(format!(
            "no plugin named {name:?} in the effective config"
        )));
    }
    cfg.validate()?;
    cfg.save(&cfg_path)?;

    commit_change(store, &format!("balls: plugin disable {name}"))?;
    Ok(DisableReport { config_path: cfg_path })
}

#[derive(Debug)]
pub struct EnableReport {
    pub config_path: PathBuf,
    pub file_path: PathBuf,
    pub file_created: bool,
}

#[derive(Debug)]
pub struct DisableReport {
    pub config_path: PathBuf,
}

pub(crate) fn load_or_default(path: &Path) -> Result<ProjectConfig> {
    match ProjectConfig::load(path) {
        Ok(c) => Ok(c),
        Err(BallError::NotInitialized(NotInitKind::ConfigMissing(_))) => {
            Ok(ProjectConfig::default())
        }
        Err(e) => Err(e),
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Commit the change onto the tracker branch — `.balls/project.json`
/// (the plugins map) and the touched `.balls/plugins/*` config files
/// both live in the state checkout, so one `git add -A` + commit
/// publishes the whole change. A stealth repo has no state checkout —
/// a no-op; the operator commits its loose `.balls/` themselves.
pub(crate) fn commit_change(store: &Store, message: &str) -> Result<()> {
    if store.stealth {
        return Ok(());
    }
    let _g = store_lock::state_worktree_flock(store)?;
    let dir = store.state_repo_dir();
    if git::has_uncommitted_changes(&dir)? {
        git::git_add_all(&dir)?;
        git::git_commit(&dir, message)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "plugin_admin_tests.rs"]
mod tests;
