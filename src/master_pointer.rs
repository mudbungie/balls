//! `.balls/master.json` — the federation bootstrap pointer (bl-82a4).
//!
//! Carries only the two fields balls must read *before* the canonical
//! `.balls/config.json` resolves: `master_url` (where the hub lives)
//! and `state_remote` (legacy git-remote-name path). Everything else
//! — task policy, plugins, target-branch, delivery — lives in the
//! canonical config, which in federated mode is a symlink to the
//! hub's own `.balls/config.json`.
//!
//! The split lets bl operate without branching on `master_url` at
//! every config read: the seam is filesystem-shaped, set up at
//! `bl remaster` time. A standalone repo has no master.json (or one
//! with both fields empty) and the canonical is a regular file.
//!
//! Legacy fallback (pre-bl-82a4): a config that still has `master_url`
//! or `state_remote` set in `.balls/config.json` and no `master.json`
//! file is read transparently — `load()` synthesizes a pointer from
//! those fields. Migration into the new shape happens on the next
//! `bl remaster <url>`.

use crate::error::{BallError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Filename of the pointer, relative to `.balls/`.
const POINTER_REL: &str = "master.json";

/// Bootstrap pointer. Both fields are independently optional so a
/// pre-bl-ffb4 repo using only the legacy `state_remote` can live in
/// the new shape without a synthetic `master_url`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MasterPointer {
    /// Hub URL of an external task master. When set, balls
    /// materializes its own clone at `.balls/state-repo/` and routes
    /// every state-branch op through it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_url: Option<String>,
    /// Legacy: git remote *name* whose `balls/tasks` ref this repo
    /// negotiates against. `None` resolves to `origin`. Deprecated in
    /// favor of `master_url`; kept here so a pre-ffb4 repo migrates
    /// into the new file shape without losing its link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_remote: Option<String>,
}

impl MasterPointer {
    /// `<root>/.balls/master.json`.
    pub fn path(root: &Path) -> PathBuf {
        root.join(".balls").join(POINTER_REL)
    }

    /// Read the pointer. If `master.json` exists, it is authoritative.
    /// Otherwise fall back to the legacy in-canonical shape — pre-bl-82a4
    /// repos kept these two fields in `.balls/config.json`, and `load`
    /// synthesizes a pointer from them so the rest of bl never has to
    /// branch on file shape.
    ///
    /// Pure standalone (no pointer, no legacy fields) returns an empty
    /// pointer. The empty pointer is the standalone signal.
    pub fn load(root: &Path) -> Result<Self> {
        let p = Self::path(root);
        if p.exists() {
            let s = fs::read_to_string(&p)?;
            return serde_json::from_str(&s).map_err(BallError::from);
        }
        Ok(read_legacy(root))
    }

    /// `Self::load` that swallows IO/parse errors and returns an empty
    /// pointer. Used in early-bootstrap paths (state-worktree resolution,
    /// auto-provisioning) where a corrupt/missing file should fall
    /// through to the caller's subsequent diagnostic, not crash here.
    pub fn load_or_empty(root: &Path) -> Self {
        Self::load(root).unwrap_or_default()
    }

    /// Persist the pointer. Writes `.balls/master.json` (creating the
    /// `.balls/` parent dir if needed). Both fields empty ⇒ removes
    /// the file rather than writing `{}` — keeps a detached repo's
    /// `.balls/` clean of zero-content state.
    pub fn save(&self, root: &Path) -> Result<()> {
        let p = Self::path(root);
        if self.is_empty() {
            if p.exists() {
                fs::remove_file(&p)?;
            }
            return Ok(());
        }
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&p, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    /// `true` when this pointer carries no federation signal — neither
    /// a `master_url` nor a `state_remote`. Standalone repos have an
    /// empty pointer (or no file at all).
    pub fn is_empty(&self) -> bool {
        self.master_url.is_none() && self.state_remote.is_none()
    }

    /// `master_url` as `&str` for the federation-bootstrap call sites.
    pub fn master_url(&self) -> Option<&str> {
        self.master_url.as_deref()
    }

    /// Resolved git-remote name for legacy `state_remote` mode. `None`
    /// ⇒ `origin`, byte-identical to a single-repo setup.
    pub fn state_remote(&self) -> &str {
        self.state_remote.as_deref().unwrap_or("origin")
    }
}

/// Read the legacy in-canonical shape. A pre-bl-82a4 repo's
/// `.balls/config.json` carries `master_url` / `state_remote` directly;
/// extract them so callers see a uniform `MasterPointer` regardless of
/// the on-disk migration state. A missing/unreadable canonical is
/// "standalone" — the caller's later `Config::load` surfaces the real
/// diagnostic if any.
fn read_legacy(root: &Path) -> MasterPointer {
    let p = root.join(".balls").join("config.json");
    let Ok(s) = fs::read_to_string(&p) else {
        return MasterPointer::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
        return MasterPointer::default();
    };
    MasterPointer {
        master_url: v.get("master_url").and_then(|x| x.as_str()).map(String::from),
        state_remote: v.get("state_remote").and_then(|x| x.as_str()).map(String::from),
    }
}

#[cfg(test)]
#[path = "master_pointer_tests.rs"]
mod tests;
