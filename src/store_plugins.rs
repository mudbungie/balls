//! "Master wins" plugin config layering (bl-a7d9).
//!
//! When `master_url` is set, the **hub** is authoritative for plugin
//! policy: the project-side `plugins` map and `.balls/plugins/`
//! directory are ignored, and the effective values come from the
//! state-worktree's `.balls/config.json` (and the `.balls/plugins/`
//! directory beside it). The seam consolidates the rule — every caller
//! of `Store::load_config()` sees the right map, and `Plugin::resolve`
//! routes per-plugin config files through `plugin_config_root()`.
//!
//! Standalone repos (no master_url) keep reading plugin policy from
//! the project's committed config — byte-identical to pre-bl-a7d9.

use crate::config::{Config, PluginEntry};
use crate::error::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Load the project's `.balls/config.json`, then — under `master_url`
/// — replace the `plugins` map with the hub-side state-worktree's
/// view. Project-side plugin drift (a non-empty map or any real file
/// under `.balls/plugins/`) is warned about once per process so the
/// operator knows to migrate.
pub(crate) fn load_layered(
    config_path: &Path,
    state_worktree: &Path,
    project_root: &Path,
) -> Result<Config> {
    let (cfg, drifted) = load_layered_inner(config_path, state_worktree, project_root)?;
    if drifted {
        warn_drift_once();
    }
    Ok(cfg)
}

/// Pure layering. Returns the effective config and a flag indicating
/// whether the project-side plugin config we just discarded carried
/// content. Side effects (the warning) live in the wrapper so this
/// stays testable.
fn load_layered_inner(
    config_path: &Path,
    state_worktree: &Path,
    project_root: &Path,
) -> Result<(Config, bool)> {
    let mut cfg = Config::load(config_path)?;
    if cfg.master_url().is_none() {
        return Ok((cfg, false));
    }
    let drifted = !cfg.plugins.is_empty() || has_real_plugin_files(project_root);
    cfg.plugins = read_state_plugins(state_worktree)?.unwrap_or_default();
    Ok((cfg, drifted))
}

/// Directory containing the *effective* `.balls/plugins/` for this
/// clone. Hub-rooted under `master_url`, project-rooted otherwise.
/// The path concatenated with a `PluginEntry::config_file` value is
/// the absolute config-file path the plugin runner hands to the
/// subprocess.
pub(crate) fn plugin_config_root(
    project_root: &Path,
    state_worktree: &Path,
    cfg: &Config,
) -> PathBuf {
    if cfg.master_url().is_some() {
        state_worktree.to_path_buf()
    } else {
        project_root.to_path_buf()
    }
}

/// Store-aware shim used by `Plugin::resolve`. Re-reads the project's
/// committed config to detect `master_url` — independent of the
/// layered `load_config()` so we never recurse into a hub read. A
/// malformed/missing config falls back to defaults (no master_url ⇒
/// project-rooted), letting the caller's later `load_config()` surface
/// the real diagnostic.
pub(crate) fn plugin_config_root_for_store(store: &crate::store::Store) -> PathBuf {
    let cfg = Config::load(&store.config_path()).unwrap_or_default();
    plugin_config_root(&store.root, &store.state_worktree_dir(), &cfg)
}

fn read_state_plugins(state_worktree: &Path) -> Result<Option<BTreeMap<String, PluginEntry>>> {
    let p = state_worktree.join(".balls/config.json");
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(Config::load(&p)?.plugins))
}

/// `true` when the project's `.balls/plugins/` has any non-placeholder
/// entry — i.e., committed plugin JSON we're about to ignore.
fn has_real_plugin_files(project_root: &Path) -> bool {
    let dir = project_root.join(".balls/plugins");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return false;
    };
    rd.flatten()
        .any(|e| e.file_name().to_str().is_some_and(|n| n != ".gitkeep"))
}

static WARNED: AtomicBool = AtomicBool::new(false);

fn warn_drift_once() {
    emit_drift_warning(&WARNED);
}

/// Emit the master-wins drift warning the first time `once` flips
/// from `false` to `true`; later calls with the same flag are silent.
/// Factored to take the flag so tests can exercise both arms with a
/// fresh `AtomicBool` without touching process state.
fn emit_drift_warning(once: &AtomicBool) {
    if once.swap(true, Ordering::Relaxed) {
        return;
    }
    eprintln!(
        "warning: master_url is set; project-side plugin config is ignored \
         (master wins, bl-a7d9). Move plugin entries and `.balls/plugins/*` \
         to the hub's `balls/tasks` branch."
    );
}

#[cfg(test)]
#[path = "store_plugins_tests.rs"]
mod tests;
