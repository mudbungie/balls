//! `bl plugin enable/disable/list` — first-class plugin management
//! (bl-32e5). Honors the `master_url` master-wins seam (bl-a7d9): in
//! master_url mode the writes land in the state-repo's checkout on
//! `balls/tasks`; in standalone mode they update the project's
//! committed `.balls/config.json` in place. The standalone variant
//! deliberately stops short of `git commit` — the project's main
//! branch belongs to the operator (see also `bl remaster`'s `--commit`
//! shape) — and prints a one-line follow-up hint instead.

use crate::config::{Config, PluginEntry};
use crate::error::{BallError, NotInitKind, Result};
use crate::git;
use crate::store::Store;
use crate::store_lock;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the effective plugins map lives. Routes through the same seam
/// as `Plugin::resolve`'s config-file lookup (`store.plugin_config_root`)
/// so the admin surface and the runtime always agree.
pub fn effective_config_path(store: &Store) -> PathBuf {
    store.plugin_config_root().join(".balls/config.json")
}

pub fn effective_plugins_dir(store: &Store) -> PathBuf {
    store.plugin_config_root().join(".balls/plugins")
}

/// Provenance of the effective plugins map shown by `bl plugin list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Project,
    Hub,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Project => "project",
            Source::Hub => "hub",
        }
    }

    pub(crate) fn from_store(store: &Store) -> Self {
        if crate::master_pointer::MasterPointer::load_or_empty(&store.root)
            .master_url()
            .is_some()
        {
            Source::Hub
        } else {
            Source::Project
        }
    }
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

/// Effective plugins map plus its source. Single read used by both
/// `list` and the enable/disable mutators to surface drift errors at
/// the same moment.
pub fn load_effective(store: &Store) -> Result<(BTreeMap<String, PluginEntry>, Source)> {
    let cfg = load_or_default(&effective_config_path(store))?;
    Ok((cfg.plugins, Source::from_store(store)))
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

    let source = Source::from_store(store);
    commit_change(
        store,
        source,
        &[
            Path::new(".balls/config.json"),
            &PathBuf::from(".balls/plugins").join(&resolved_file),
        ],
        &format!("balls: plugin enable {name}"),
    )?;
    Ok(EnableReport {
        source,
        config_path: cfg_path,
        file_path,
        file_created,
    })
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

    let source = Source::from_store(store);
    commit_change(
        store,
        source,
        &[Path::new(".balls/config.json")],
        &format!("balls: plugin disable {name}"),
    )?;
    Ok(DisableReport {
        source,
        config_path: cfg_path,
    })
}

#[derive(Debug)]
pub struct EnableReport {
    pub source: Source,
    pub config_path: PathBuf,
    pub file_path: PathBuf,
    pub file_created: bool,
}

#[derive(Debug)]
pub struct DisableReport {
    pub source: Source,
    pub config_path: PathBuf,
}

pub(crate) fn load_or_default(path: &Path) -> Result<Config> {
    match Config::load(path) {
        Ok(c) => Ok(c),
        Err(BallError::NotInitialized(NotInitKind::ConfigMissing(_))) => Ok(Config::default()),
        Err(e) => Err(e),
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// In master_url mode commit the change on the state-repo's
/// `balls/tasks` checkout — the same seam tasks use. In standalone
/// mode this is a no-op: the project's `.balls/config.json` belongs
/// to whichever branch is currently checked out and the operator
/// commits it themselves (matching `bl remaster --commit`).
pub(crate) fn commit_change(
    store: &Store,
    source: Source,
    paths: &[&Path],
    message: &str,
) -> Result<()> {
    if source != Source::Hub || store.stealth {
        return Ok(());
    }
    let _g = store_lock::state_worktree_flock(store)?;
    let dir = store.state_worktree_dir();
    git::git_add(&dir, paths)?;
    git::git_commit(&dir, message)?;
    Ok(())
}

#[cfg(test)]
#[path = "plugin_admin_tests.rs"]
mod tests;
