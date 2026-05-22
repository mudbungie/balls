//! Plugin-config root resolver + legacy "master wins" compat layer.
//!
//! Post bl-82a4 the federation seam lives in the filesystem: in a
//! migrated federated repo `.balls/config.json` is a symlink to the
//! hub's canonical and `.balls/plugins/` is a symlink to the hub's
//! plugins directory. The runtime reads transparently through both.
//!
//! Legacy compat: a federated repo that hasn't been migrated yet
//! (master_url present in committed config or master.json, but the
//! canonical and plugins/ are still regular files) keeps the pre-
//! bl-a7d9 "master wins" behavior — we layer the hub-side plugins
//! over the project-side config at read time. The fix-it path is
//! `bl remaster <url> --commit`, which materializes the symlinks
//! and removes this branch.

use crate::config::{Config, PluginEntry};
use crate::error::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `Store::load_config`: returns the effective config, applying the
/// legacy layering only for unmigrated federated repos. A symlinked
/// canonical reads through directly — no layering needed.
pub(crate) fn load_effective(
    config_path: &Path,
    state_worktree: &Path,
    pointer_has_master_url: bool,
) -> Result<Config> {
    let mut cfg = Config::load(config_path)?;
    if !pointer_has_master_url {
        return Ok(cfg);
    }
    if config_path.is_symlink() {
        // Modern federated: the symlink already points at the hub's
        // canonical, so cfg.plugins is hub-side by construction.
        return Ok(cfg);
    }
    // Legacy federated: project-side regular file; layer hub-side
    // plugins on top exactly as bl-a7d9 did.
    if let Some(hub) = read_state_plugins(state_worktree)? {
        cfg.plugins = hub;
    }
    Ok(cfg)
}

/// Store-aware shim used by `Plugin::resolve`. Returns the root that
/// `PluginEntry::config_file` (a `.balls/plugins/<x>.json`-relative
/// path) is joined against to produce the absolute config-file path.
pub(crate) fn plugin_config_root_for_store(store: &crate::store::Store) -> PathBuf {
    let plugins_symlinked = store.root.join(".balls").join("plugins").is_symlink();
    let has_master = crate::master_pointer::MasterPointer::load_or_empty(&store.root)
        .master_url()
        .is_some();
    plugin_config_root(&store.root, &store.state_worktree_dir(), plugins_symlinked, has_master)
}

/// Pure root resolution. In a migrated federated repo the project
/// root works — the `.balls/plugins/` symlink does the redirect.
/// Only an *unmigrated* legacy federated repo (master_url set but
/// `.balls/plugins/` still a real dir) needs the explicit hub root.
fn plugin_config_root(
    root: &Path,
    state_worktree: &Path,
    plugins_symlinked: bool,
    has_master: bool,
) -> PathBuf {
    if has_master && !plugins_symlinked {
        return state_worktree.to_path_buf();
    }
    root.to_path_buf()
}

fn read_state_plugins(state_worktree: &Path) -> Result<Option<BTreeMap<String, PluginEntry>>> {
    let p = state_worktree.join(".balls/config.json");
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(Config::load(&p)?.plugins))
}

#[cfg(test)]
#[path = "store_plugins_tests.rs"]
mod tests;
