//! §2/§6 plugin registry — the filesystem *is* the registry.
//!
//! On the `balls` branch, `config/plugins/` wires which plugin runs in which
//! op-phase, with a two-level stitch that keeps the wiring portable while the
//! actual binary stays machine-local:
//!
//! ```text
//! config/plugins/<op>/<phase>/NN-<name>   COMMITTED relative symlink → ../../bin/<name>
//! config/plugins/bin/<name>               LOCAL (gitignored) absolute symlink → the binary
//! ```
//!
//! The committed half travels with the branch and is internal-relative, so it
//! is valid wherever the checkout sits (stealth or federation). The local half
//! is resolved per machine by `bl install`; a missing `bin/<name>` is a clean
//! "referenced but not installed here" — surfaced as a dangling [`PluginRef`],
//! never a silent skip. An absent or empty phase dir means *run nothing* (§4):
//! the no-plugins case is the general path with no entries, not a special case.
//!
//! (§6 writes the relative target as `../bin/<name>`; with the literal
//! `<op>/<phase>/` depth the path that actually resolves is `../../bin/<name>`,
//! which is what [`Registry::link`] emits.)

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// One wired plugin in an op-phase: its run order, its name, and the resolved
/// machine binary — `None` when `bin/<name>` is absent or dangling here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRef {
    pub order: u32,
    pub name: String,
    pub bin: Option<PathBuf>,
}

/// The `config/plugins/` subtree of one operating checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    plugins: PathBuf,
}

impl Registry {
    /// Locate the registry under an operating checkout's `config/plugins/`.
    #[must_use]
    pub fn at(operating: &Path) -> Self {
        Self {
            plugins: operating.join("config").join("plugins"),
        }
    }

    fn phase_dir(&self, op: &str, phase: &str) -> PathBuf {
        self.plugins.join(op).join(phase)
    }

    fn bin_dir(&self) -> PathBuf {
        self.plugins.join("bin")
    }

    /// Wire `name` into `<op>/<phase>/` at run-order `order`: the COMMITTED
    /// relative symlink `NN-<name>` → `../../bin/<name>`. Idempotent — an
    /// existing link at that slot is replaced.
    pub fn link(&self, op: &str, phase: &str, order: u32, name: &str) -> io::Result<()> {
        let dir = self.phase_dir(op, phase);
        fs::create_dir_all(&dir)?;
        let target = Path::new("..").join("..").join("bin").join(name);
        replace_symlink(&target, &dir.join(format!("{order:02}-{name}")))
    }

    /// Bind `name` to this machine's `target` binary: the LOCAL (gitignored)
    /// absolute symlink `bin/<name>` → `target`. Idempotent — re-binding
    /// replaces the existing link.
    pub fn bind(&self, name: &str, target: &Path) -> io::Result<()> {
        let dir = self.bin_dir();
        fs::create_dir_all(&dir)?;
        replace_symlink(target, &dir.join(name))
    }

    /// The plugins wired into `<op>/<phase>/`, in `NN-` run order. An absent or
    /// empty phase dir yields an empty list (run nothing, §4). Each ref's `bin`
    /// resolves `bin/<name>` to its real path, or `None` if not installed here.
    pub fn resolve(&self, op: &str, phase: &str) -> io::Result<Vec<PluginRef>> {
        let dir = self.phase_dir(op, phase);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut refs = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if let Some((order, name)) = parse_entry(&entry.file_name().to_string_lossy()) {
                let bin = fs::canonicalize(self.bin_dir().join(&name)).ok();
                refs.push(PluginRef { order, name, bin });
            }
        }
        refs.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
        Ok(refs)
    }
}

/// Parse a registry entry filename `NN-<name>` into `(order, name)`. A name
/// with no `-`, an empty name, or a non-numeric prefix is not an entry. Shared
/// with [`crate::install`], which reads the same `NN-<name>` wiring it copies.
pub(crate) fn parse_entry(file_name: &str) -> Option<(u32, String)> {
    let (prefix, rest) = file_name.split_once('-')?;
    if rest.is_empty() {
        return None;
    }
    let order: u32 = prefix.parse().ok()?;
    Some((order, rest.to_string()))
}

/// Create `link` → `original`, first removing any path already at `link` so
/// re-linking is idempotent. Removing a symlink never touches its target.
/// Shared with [`crate::install`], whose plugins-object copy re-creates each
/// committed relative symlink (idempotent replace = innermost wins, §6).
pub(crate) fn replace_symlink(original: &Path, link: &Path) -> io::Result<()> {
    if link.symlink_metadata().is_ok() {
        fs::remove_file(link)?;
    }
    symlink(original, link)
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
