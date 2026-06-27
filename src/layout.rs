//! §1 XDG layout — where balls' host-side state lives, as pure path arithmetic.
//!
//! Two coordinate roots, both under `balls/`:
//!
//! ```text
//! $XDG_CONFIG_HOME/balls/config.toml          # user-level config
//! $XDG_STATE_HOME/balls/
//!   plugins/<name>/                           # each plugin owns this subtree
//!   clones/<pct-enc-invocation-path>/         # one bundle per invocation path
//!     binding.toml                            #   tracker remote + invocation_path + tasks_branch
//!     config/                                 #   the LANDING — balls/config checkout (§2)
//!     tasks/                                  #   the STORE — tasks_branch checkout (§2)
//!     changes/<uuid>/                         #   in-flight CHANGE worktrees (§8)
//!     log                                     #   the unified op log (JSON-lines, §6)
//! ```
//!
//! No env reads here: the binary edge resolves `HOME` and the XDG variables
//! once and hands them in (the bl-bfa8 lesson — env reads at the edge so
//! parallel tests vary the layout without racing). No `mkdir`: this layer
//! answers *where*, never *make it so*. Per §0, core gives a plugin only its
//! territory root ([`Xdg::plugin_territory`]); the tracker's `<pct-enc-remote>/`
//! is that plugin's own business, built from the [`crate::encoding`] primitive.
//! The delivery plugin's `<invocation-path>/<id>/` is the lone exception: it
//! MIRRORS the path rather than encoding it, because that subtree is a `cargo`
//! build dir and `rust-lld` chokes on a `%` in an output path (bl-f3e4 — see
//! [`crate::delivery::binding_territory`]).

use crate::encoding::percent_encode;
use std::path::{Path, PathBuf};

/// The two XDG base directories balls roots its state under. Built once at the
/// binary edge from `HOME` + the XDG variables via [`Xdg::with`]; an absent or
/// empty variable falls back to its XDG-spec default under `home`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Xdg {
    config_home: PathBuf,
    state_home: PathBuf,
}

impl Xdg {
    /// Resolve the bases from `home` plus `$XDG_CONFIG_HOME` / `$XDG_STATE_HOME`
    /// (each `None` or empty falling back to `~/.config` / `~/.local/state`).
    /// Pure — no env reads, no I/O.
    #[must_use]
    pub fn with(home: &Path, config_home: Option<&str>, state_home: Option<&str>) -> Self {
        Self {
            config_home: resolve_base(home, ".config", config_home),
            state_home: resolve_base(home, ".local/state", state_home),
        }
    }

    /// `$XDG_CONFIG_HOME/balls/config.toml` — the user-level config layer (§4).
    #[must_use]
    pub fn user_config(&self) -> PathBuf {
        self.config_home.join("balls").join("config.toml")
    }

    /// `$XDG_CONFIG_HOME/balls/default-config/` — a DELIBERATE seed override
    /// (§1/§12). Present = an org/user customizes the seed (its files win
    /// per-file); absent = the embedded default is used directly. Core NEVER
    /// creates it ([`crate::seed`], bl-8088), so a once-materialized copy can't go
    /// stale and shadow the embedded default.
    #[must_use]
    pub fn default_config(&self) -> PathBuf {
        self.config_home.join("balls").join("default-config")
    }

    /// `$XDG_STATE_HOME/balls/` — the single state root for every clone and
    /// plugin on this host.
    #[must_use]
    pub fn state_dir(&self) -> PathBuf {
        self.state_home.join("balls")
    }

    /// `$XDG_STATE_HOME/balls/plugins/<name>/` — the one subtree a plugin owns.
    /// Core hands over the root and reads nothing inside it (§0).
    #[must_use]
    pub fn plugin_territory(&self, name: &str) -> PathBuf {
        self.state_dir().join("plugins").join(name)
    }

    /// The clone bundle for one invocation path, percent-encoded into a single
    /// component: `$XDG_STATE_HOME/balls/clones/<pct-enc-invocation-path>/`.
    #[must_use]
    pub fn clone_dir(&self, invocation_path: &Path) -> CloneDir {
        let enc = percent_encode(&invocation_path.to_string_lossy());
        CloneDir {
            root: self.state_dir().join("clones").join(enc),
        }
    }
}

fn resolve_base(home: &Path, default_rel: &str, xdg: Option<&str>) -> PathBuf {
    match xdg.filter(|s| !s.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => home.join(default_rel),
    }
}

/// The per-invocation-path bundle `clones/<pct-enc-invocation-path>/` and the
/// things that live in it. Pure paths; the change worktree is core and
/// uuid-named (nothing keys off the uuid — §1), distinct from the delivery
/// plugin's code worktree in plugin territory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloneDir {
    root: PathBuf,
}

impl CloneDir {
    /// The bundle root itself.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `binding.toml` — the tracker remote (if any) + invocation path + the
    /// `tasks_branch` the store rides (§1).
    #[must_use]
    pub fn binding(&self) -> PathBuf {
        self.root.join("binding.toml")
    }

    /// `config/` — the LANDING checkout (the `balls/config` branch, §2). A real
    /// worktree balls reads config from; this layer only names the path. `config/`
    /// is a top-level folder ALWAYS (§2), whatever else a branch carries.
    #[must_use]
    pub fn landing(&self) -> PathBuf {
        self.root.join("config")
    }

    /// `tasks/` — the STORE checkout (the `tasks_branch` branch, §2). The linked
    /// worktree balls reads/writes tasks on and seals task ops onto (§8). `tasks/`
    /// is a top-level folder ALWAYS (§2). The two checkouts are worktrees of ONE
    /// repo, so `tasks_branch` can never name the landing branch — git refuses a
    /// branch checked out twice; the coincident name is refused by name (bl-ac89).
    #[must_use]
    pub fn store(&self) -> PathBuf {
        self.root.join("tasks")
    }

    /// `changes/<uuid>/` — one ephemeral CHANGE worktree for an in-flight op
    /// (§8). The caller supplies the uuid; nothing keys off it.
    #[must_use]
    pub fn change(&self, uuid: &str) -> PathBuf {
        self.root.join("changes").join(uuid)
    }

    /// `log` — the unified per-clone op log (§1/§6): JSON-lines, balls-owned, the
    /// single sink for core lifecycle records and enveloped plugin stderr. Local
    /// runtime state, gitignored, never committed (like [`Self::binding`]).
    #[must_use]
    pub fn op_log(&self) -> PathBuf {
        self.root.join("log")
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
