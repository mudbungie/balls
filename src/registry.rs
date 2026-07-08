//! §6 local binary binding — `config/plugins/bin/<name>`.
//!
//! The hook SCHEDULE is committed text in `config/plugins.toml` ([`crate::hooks`]);
//! this module owns the other half of the two-level stitch: the LOCAL, gitignored
//! `config/plugins/bin/<name>` — an ABSOLUTE symlink to this machine's binary.
//!
//! ```text
//! config/plugins.toml         COMMITTED [hooks] schedule (names, portable)
//! config/plugins/bin/<name>   LOCAL (gitignored) absolute symlink → the binary
//! ```
//!
//! The schedule travels with the branch / `bl install` and is machine-neutral;
//! the binding is resolved per machine ([`Registry::bind`], by `bl prime`'s seed
//! and `bl install`). [`Registry::resolve_bin`] reads it back; a missing
//! `bin/<name>` is a clean "referenced but not installed here" (`None`), surfaced
//! by the dispatch when a hooked name has no binary, never a silent skip.

use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_file as symlink;
use std::path::{Path, PathBuf};

/// One wired plugin: its name (from the committed [`crate::hooks`] schedule) and
/// the resolved machine binary — `None` when `bin/<name>` is absent or dangling.
/// `source` is the name's `[source]` acquisition hint (bl-5b09) — display-only
/// text the unbound refusal appends; never a dispatch input (the binding is
/// bit-identical with or without it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRef {
    pub name: String,
    pub bin: Option<PathBuf>,
    pub source: Option<String>,
}

/// The `config/plugins/bin/` binding store of one landing checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    plugins: PathBuf,
}

impl Registry {
    /// Locate the binding store under a landing checkout's `config/plugins/`.
    #[must_use]
    pub fn at(landing: &Path) -> Self {
        Self {
            plugins: landing.join("config").join("plugins"),
        }
    }

    fn bin_dir(&self) -> PathBuf {
        self.plugins.join("bin")
    }

    /// Bind `name` to this machine's `target` binary: the LOCAL (gitignored)
    /// absolute symlink `bin/<name>` → `target`. Idempotent — re-binding replaces
    /// the existing link.
    pub fn bind(&self, name: &str, target: &Path) -> io::Result<()> {
        let dir = self.bin_dir();
        fs::create_dir_all(&dir)?;
        replace_symlink(target, &dir.join(name))
    }

    /// The machine binary `name` resolves to here, or `None` when `bin/<name>` is
    /// absent, dangling, or not a regular file (the "referenced but not installed"
    /// case, §6 — an empty name joins to `bin/` itself, a directory, bl-bee0).
    #[must_use]
    pub fn resolve_bin(&self, name: &str) -> Option<PathBuf> {
        fs::canonicalize(self.bin_dir().join(name)).ok().filter(|p| p.is_file())
    }

    /// Drop `name`'s local binding if one is present — the DANGLING symlink an
    /// old prime left when a first-party plugin was renamed away (§15 converge,
    /// bl-18bf): a dangling link is not work, so removing it is safe. Absent ⇒
    /// nothing to do; removing a symlink never touches its (already-gone) target.
    pub fn unbind(&self, name: &str) -> io::Result<()> {
        let link = self.bin_dir().join(name);
        if link.symlink_metadata().is_ok() {
            fs::remove_file(link)?;
        }
        Ok(())
    }
}

/// Create `link` → `original`, first removing any path already at `link` so
/// re-linking is idempotent. Removing a symlink never touches its target.
pub(crate) fn replace_symlink(original: &Path, link: &Path) -> io::Result<()> {
    if link.symlink_metadata().is_ok() {
        fs::remove_file(link)?;
    }
    symlink(original, link)
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
