//! XDG path layer for the nested layout per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §3.
//!
//! Pure path arithmetic: given XDG bases plus the three identifiers
//! from `crate::encoding` (`<enc-origin>`, `<enc-branch>`,
//! `<nested-clone-path>`), this module returns the documented paths
//! under `~/.local/state/balls/` and `~/.config/balls/`. No I/O, no
//! `mkdir`, no env reads — callers supply the bases. The `bl-bfa8`
//! lesson holds (env reads belong at the binary edge so parallel
//! tests can vary the layout without racing).
//!
//! What this layer rejects from the previous SPEC revision:
//! - No `<origin-key>` or `<path-hash>` parent directories — the new
//!   layout is flat under `state/balls/{trackers,worktrees,…}/`.
//! - No `active/`/`own/` view-symlink trees — reads go to the tracker
//!   checkout paths directly (SPEC §13: "No view symlinks").
//! - No `~/.cache/balls/<key>/` per-repo subtree — the cache root is
//!   reserved-but-empty in this layout (SPEC §3).

use crate::encoding::ENC_BALLS_TASKS;
use std::path::{Path, PathBuf};

/// XDG base directories. Pass `home`, `$XDG_CONFIG_HOME`,
/// `$XDG_STATE_HOME`, `$XDG_CACHE_HOME` once at the binary edge via
/// [`XdgBases::from_env`]; tests build one directly with
/// [`XdgBases::with`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XdgBases {
    pub config_home: PathBuf,
    pub state_home: PathBuf,
    pub cache_home: PathBuf,
}

impl XdgBases {
    /// Construct from `(home, $XDG_CONFIG_HOME, $XDG_STATE_HOME,
    /// $XDG_CACHE_HOME)`. An empty or absent XDG variable falls back
    /// to the XDG-spec default under `home`. Pure function — no env
    /// reads.
    #[must_use]
    pub fn with(
        home: &Path,
        config_home: Option<&str>,
        state_home: Option<&str>,
        cache_home: Option<&str>,
    ) -> Self {
        Self {
            config_home: resolve_base(home, ".config", config_home),
            state_home: resolve_base(home, ".local/state", state_home),
            cache_home: resolve_base(home, ".cache", cache_home),
        }
    }

    /// Read `HOME` and the three XDG variables from the process
    /// environment, applying the same fallback rules as
    /// [`Self::with`]. Returns `None` if `HOME` is unset or empty —
    /// the caller decides what to do (the binary edge errors with a
    /// clear "no HOME" diagnostic; stealth callers fall through to a
    /// `clone.json`-based path).
    pub fn from_env() -> Option<Self> {
        Self::from_strings(
            std::env::var("HOME").ok(),
            std::env::var("XDG_CONFIG_HOME").ok(),
            std::env::var("XDG_STATE_HOME").ok(),
            std::env::var("XDG_CACHE_HOME").ok(),
        )
    }

    /// Pure version of [`Self::from_env`] — same logic, env values
    /// passed in as `Option<String>`. Exposed so tests can exercise
    /// the `None`-HOME branch without mutating the process
    /// environment (bl-bfa8 parallel-test race).
    #[must_use]
    pub fn from_strings(
        home: Option<String>,
        config: Option<String>,
        state: Option<String>,
        cache: Option<String>,
    ) -> Option<Self> {
        let home = home.filter(|s| !s.is_empty())?;
        Some(Self::with(
            Path::new(&home),
            config.as_deref(),
            state.as_deref(),
            cache.as_deref(),
        ))
    }

    /// `~/.local/state/balls/`. The single state root for all
    /// clones on this host.
    #[must_use]
    pub fn state_root(&self) -> PathBuf {
        self.state_home.join("balls")
    }

    /// `~/.config/balls/`. Only `clone.json` ever lives here, under
    /// `<nested-clone-path>/`.
    #[must_use]
    pub fn config_root(&self) -> PathBuf {
        self.config_home.join("balls")
    }

    /// `~/.cache/balls/`. SPEC §3: reserved for derived/regenerable
    /// artifacts; empty in this layout.
    #[must_use]
    pub fn cache_root(&self) -> PathBuf {
        self.cache_home.join("balls")
    }
}

fn resolve_base(home: &Path, default_rel: &str, xdg: Option<&str>) -> PathBuf {
    match xdg.filter(|s| !s.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => home.join(default_rel),
    }
}

/// The tracker checkout path under
/// `~/.local/state/balls/trackers/<enc-origin>/<enc-branch>/`.
///
/// The single source of truth for "where the tracker's `balls/tasks`
/// branch is checked out on disk." Two clones of the same origin
/// share this directory; their tracker fetches are shared on the
/// host. SPEC §3, §4.
#[must_use]
pub fn tracker_checkout(bases: &XdgBases, enc_origin: &str, enc_branch: &str) -> PathBuf {
    bases
        .state_root()
        .join("trackers")
        .join(enc_origin)
        .join(enc_branch)
}

/// Convenience: tracker checkout for the bootstrap branch
/// (`balls/tasks` → `balls%2Ftasks`). Used everywhere the bootstrap
/// hop is involved — the common case for a non-federated clone.
#[must_use]
pub fn own_tracker_checkout(bases: &XdgBases, enc_origin: &str) -> PathBuf {
    tracker_checkout(bases, enc_origin, ENC_BALLS_TASKS)
}

/// All per-clone directories under
/// `~/.local/state/balls/{worktrees,claims,locks,plugins-auth}/<nested>/`.
/// One bundle so the layout's per-clone surface is a single
/// constructor — discover/init/claim never compute these paths
/// individually.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerClonePaths {
    pub worktrees: PathBuf,
    pub claims: PathBuf,
    pub locks: PathBuf,
    pub plugins_auth: PathBuf,
}

impl PerClonePaths {
    /// Build the four per-clone roots for a given `<nested-clone-path>`.
    /// The four siblings share the same nested-path suffix; the
    /// peer directories are flat (no shared `<origin-key>` parent).
    #[must_use]
    pub fn new(bases: &XdgBases, nested_clone_path: &Path) -> Self {
        let state = bases.state_root();
        Self {
            worktrees: state.join("worktrees").join(nested_clone_path),
            claims: state.join("claims").join(nested_clone_path),
            locks: state.join("locks").join(nested_clone_path),
            plugins_auth: state.join("plugins-auth").join(nested_clone_path),
        }
    }

    /// Path for one task's worktree:
    /// `<worktrees>/<bl-id>/`. SPEC §3 / §8.
    #[must_use]
    pub fn worktree_for(&self, bl_id: &str) -> PathBuf {
        self.worktrees.join(bl_id)
    }
}

/// Path to a clone's per-on-disk-checkout config file:
/// `~/.config/balls/<nested-clone-path>/clone.json`. SPEC §3 / §6.4.
#[must_use]
pub fn clone_json_path(bases: &XdgBases, nested_clone_path: &Path) -> PathBuf {
    bases
        .config_root()
        .join(nested_clone_path)
        .join("clone.json")
}

#[cfg(test)]
#[path = "xdg_paths_tests.rs"]
mod tests;
