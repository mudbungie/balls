//! "Master wins" plugin config layering (bl-a7d9, bl-1098).
//!
//! When `master_url` is set, the **hub** is authoritative for plugin
//! policy. Two seams encode that:
//!   1. The `plugins` map in `.balls/config.json` is replaced at
//!      load time with the state-worktree's view (this module's
//!      `load_layered`). `plugin_config_root` routes config.json
//!      reads/writes to the hub. Pending bl-82a4, config.json itself
//!      is not yet a symlink.
//!   2. The per-plugin config files under `.balls/plugins/<name>.json`
//!      are *also* reachable through a `.balls/plugins -> state-repo/
//!      .balls/plugins` symlink at the project root, materialized by
//!      `state_repo::ensure` (bl-1098, option a — two parallel
//!      symlinks, not an umbrella path). The symlink keeps on-disk
//!      layout honest with the master-wins rule and lets engineers
//!      `$EDITOR .balls/plugins/<name>.json` without knowing where
//!      the hub lives.
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

/// Directory containing the *effective* `.balls/` for this clone:
/// hub-rooted under `master_url`, project-rooted otherwise. Joining a
/// `PluginEntry::config_file` value yields the absolute config-file
/// path the plugin runner hands to the subprocess; joining
/// `.balls/config.json` yields the effective plugins-map source for
/// `bl plugin enable/disable/list`.
///
/// For per-plugin file reads under bl-1098, the `.balls/plugins`
/// symlink at the project root resolves to the same files — so callers
/// could equivalently use the project root. The branching stays here
/// because `.balls/config.json` itself isn't yet a symlink (bl-82a4).
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

/// Store-aware shim used by `Plugin::resolve` and `plugin_admin`.
/// Re-reads the project's committed config to detect `master_url` —
/// independent of the layered `load_config()` so we never recurse into
/// a hub read.
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

/// `true` when the project's `.balls/plugins/` is a **real directory**
/// (not the bl-1098 symlink) holding non-placeholder entries — i.e., a
/// repo that's been flipped to `master_url` but hasn't run
/// `state_repo::ensure` yet (so the symlink isn't in place) and has
/// committed plugin JSON the hub would shadow. Symlinked dirs are by
/// definition the hub's view; no drift to warn about.
fn has_real_plugin_files(project_root: &Path) -> bool {
    let dir = project_root.join(".balls/plugins");
    if dir.is_symlink() {
        return false;
    }
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
