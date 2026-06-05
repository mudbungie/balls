//! §6 `bl install` — capability transfer between two `balls` branches.
//!
//! One symmetric verb copies a COMMITTED branch subtree from a `--from` ref to a
//! `--to` ref (default `terminus → landing`). Adopting (`terminus → landing`) and
//! publishing (`landing → center`, or seeding `well-configured → unconfigured`)
//! are the same verb, direction reversed. This module is the git-free heart: it
//! transfers the subtree between two materialized checkout roots and validates a
//! local binding. The git ref-materialization and the atomic seal onto `--to`
//! are the engine's job at run-wiring (the transfer stages into the `--to`
//! worktree; the [`crate::lifecycle::Engine`] commits it), so this layer reads
//! and writes plain dirs and is unit-tested on tempfiles like [`crate::change`].
//!
//! **One invariant dissolves two exclusions.** The plugins object copies only the
//! *relative-symlink wiring* under `config/plugins/` (directories + relative
//! symlinks; regular files and the `bin/` subtree are skipped). The committed
//! `<op>/<phase>/NN-<name>` entries are exactly those relative symlinks, so:
//! - `bin/<name>` (the LOCAL absolute symlinks) never travels — the recipient
//!   resolves binaries itself ([`resolve_and_bind`]); and
//! - `config/plugins/tracker/remote.toml` (a regular file holding the trail
//!   `next:` pointer) never travels — Pointer-exclusion, with no TOML parsing and
//!   no reach into plugin territory (§0). Pointers are set only by prime (§12).
//!
//! Object semantics (bl-0601): `plugins`/`config` are recommendations — the
//! incoming copy REPLACES the same paths (innermost wins). `tasks` is the single
//! store relocating, so it is a UNION of the two sides' `tasks/<id>.md` sets; a
//! same-`id` is a real conflict ([`InstallError::Conflict`]), never a silent
//! clobber. Non-destructive to `--from`: this layer only ever reads the source.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::plugin::describe;
use crate::registry::{parse_entry, replace_symlink, Registry};

/// What `bl install` copies. `plugins`/`config` replace; `tasks` merges (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Object {
    Plugins,
    Config,
    Tasks,
}

impl Object {
    /// Every object, the source the parser and tests draw on.
    pub const ALL: [Object; 3] = [Object::Plugins, Object::Config, Object::Tasks];

    /// The recommended bundle when `<object>` is omitted: the capability wiring,
    /// NOT `tasks` (the migration object, requested explicitly — §6).
    pub const DEFAULT_BUNDLE: [Object; 2] = [Object::Plugins, Object::Config];

    /// The canonical token — the inverse of [`Object::parse`].
    pub fn token(self) -> &'static str {
        match self {
            Object::Plugins => "plugins",
            Object::Config => "config",
            Object::Tasks => "tasks",
        }
    }

    /// Resolve a token to its object, or `None` if unrecognized.
    pub fn parse(token: &str) -> Option<Object> {
        Object::ALL.into_iter().find(|o| o.token() == token)
    }
}

/// Which plugins the copied wiring references, mapped to the op tokens they are
/// wired into — the [`resolve_and_bind`] worklist a `--to`-local install runs.
pub type Referenced = BTreeMap<String, BTreeSet<String>>;

/// Why an install could not be applied.
#[derive(Debug)]
pub enum InstallError {
    /// `install tasks` hit the same `id` on both sides — a real collision, never
    /// a silent clobber (near-impossible under random-hex ids, § id generation).
    Conflict { id: String },
    /// The resolved binary does not declare an op it is wired into, or does not
    /// speak balls' protocol version — install refuses to link it (§6).
    Unsupported { name: String, reason: String },
    /// An underlying filesystem or self-describe failure.
    Io(io::Error),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Conflict { id } => {
                write!(f, "install tasks: {id} exists on both sides — refusing to clobber")
            }
            InstallError::Unsupported { name, reason } => {
                write!(f, "install: refusing to link {name}: {reason}")
            }
            InstallError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<io::Error> for InstallError {
    fn from(e: io::Error) -> Self {
        InstallError::Io(e)
    }
}

/// Transfer each `object` from the `from` checkout root into the `to` checkout
/// root, returning the plugins the copied wiring references (op tokens included)
/// so a `--to`-local install can [`resolve_and_bind`] them. Only `plugins`
/// contributes references; `config`/`tasks` contribute none.
pub fn transfer(objects: &[Object], from: &Path, to: &Path) -> Result<Referenced, InstallError> {
    let mut refs = Referenced::new();
    for object in objects {
        match object {
            Object::Plugins => copy_wiring(from, to, &mut refs)?,
            Object::Config => copy_config(from, to)?,
            Object::Tasks => merge_tasks(from, to)?,
        }
    }
    Ok(refs)
}

/// `config/plugins/` of one checkout.
fn plugins_dir(root: &Path) -> PathBuf {
    root.join("config").join("plugins")
}

/// Copy the relative-symlink wiring under `config/plugins/`, replacing each entry
/// (innermost wins). An absent source tree means nothing is wired — copy nothing.
fn copy_wiring(from: &Path, to: &Path, refs: &mut Referenced) -> Result<(), InstallError> {
    let src = plugins_dir(from);
    if src.is_dir() {
        walk(&src, &src, &plugins_dir(to), refs)?;
    }
    Ok(())
}

/// Recurse `dir` (relative to wiring root `base`), mirroring relative symlinks
/// into `dest_root` and skipping the `bin/` subtree and every regular file.
fn walk(base: &Path, dir: &Path, dest_root: &Path, refs: &mut Referenced) -> Result<(), InstallError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base).expect("walk stays under base");
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            let target = fs::read_link(&path)?;
            if target.is_relative() {
                let dest = dest_root.join(rel);
                fs::create_dir_all(dest.parent().expect("a wiring entry always has a parent"))?;
                replace_symlink(&target, &dest)?;
                record_ref(rel, refs);
            }
        } else if file_type.is_dir() && rel != Path::new("bin") {
            walk(base, &path, dest_root, refs)?;
        }
    }
    Ok(())
}

/// Record `<op>/<phase>/NN-<name>` as plugin `name` wired into op `op` — the
/// first path component is the op, the filename the `NN-<name>` entry. A symlink
/// shallower than `<op>/<entry>`, or a filename that is not an `NN-` entry,
/// contributes no reference (it is mirrored but not resolved).
fn record_ref(rel: &Path, refs: &mut Referenced) {
    let comps: Vec<_> = rel.iter().collect();
    if comps.len() < 2 {
        return;
    }
    let op = comps[0].to_string_lossy().into_owned();
    if let Some((_, name)) = parse_entry(&comps[comps.len() - 1].to_string_lossy()) {
        refs.entry(name).or_default().insert(op);
    }
}

/// Replace `config/balls.toml` (innermost wins). An absent source = nothing to
/// recommend; copy nothing.
fn copy_config(from: &Path, to: &Path) -> Result<(), InstallError> {
    let src = from.join("config").join("balls.toml");
    if src.is_file() {
        let dest = to.join("config").join("balls.toml");
        fs::create_dir_all(dest.parent().expect("balls.toml always has a config/ parent"))?;
        fs::copy(&src, &dest)?;
    }
    Ok(())
}

/// Union-merge `tasks/<id>.md`: copy every source ball the destination lacks; a
/// ball present on both sides is a [`InstallError::Conflict`]. Non-destructive to
/// the source. An absent source `tasks/` means nothing to move.
fn merge_tasks(from: &Path, to: &Path) -> Result<(), InstallError> {
    let src = from.join("tasks");
    if !src.is_dir() {
        return Ok(());
    }
    let dest_dir = to.join("tasks");
    for entry in fs::read_dir(&src)? {
        let entry = entry?;
        let name = entry.file_name();
        if let Some(id) = name.to_string_lossy().strip_suffix(".md") {
            let dest = dest_dir.join(&name);
            if dest.exists() {
                return Err(InstallError::Conflict { id: id.to_string() });
            }
            fs::create_dir_all(&dest_dir)?;
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Resolve `name` to the local `candidate` binary and bind it (§6 two-level
/// stitch). Refuses unless the binary's `<bin> protocol` self-description
/// declares every `op` the wiring uses it for and speaks `protocol`. The binary
/// edge supplies `candidate` (a PATH lookup or an explicit `--bin name=path`),
/// keeping this layer env-free.
pub fn resolve_and_bind(
    registry: &Registry,
    name: &str,
    candidate: &Path,
    ops: &BTreeSet<String>,
    protocol: u32,
) -> Result<(), InstallError> {
    let proto = describe(candidate)?;
    if !proto.speaks(protocol) {
        return Err(InstallError::Unsupported {
            name: name.to_string(),
            reason: format!("does not speak protocol {protocol}"),
        });
    }
    if let Some(op) = ops.iter().find(|op| !proto.ops.contains(op)) {
        return Err(InstallError::Unsupported {
            name: name.to_string(),
            reason: format!("does not handle op '{op}'"),
        });
    }
    registry.bind(name, candidate)?;
    Ok(())
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
