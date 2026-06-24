//! §6 `bl install` — copy a committed path between two `balls` branches.
//!
//! Pure path-copy: `bl install <path> --from <ref> --to <ref>` makes
//! `<to>/<path>` mirror `<from>/<path>`, touching NOTHING outside `<path>`.
//! Adopting (`anvil → landing`) and publishing (`landing → center`) are the same
//! verb, direction reversed. This is the git-free heart: it copies between two
//! materialized checkout roots. The git ref-materialization and the atomic seal
//! onto `--to` are the engine's job at run-wiring (this layer stages into the
//! `--to` worktree; the [`crate::lifecycle::Engine`] commits it on top of `--to`'s
//! CURRENT tip, swapping only `<path>` — never a whole-tree replace, never a ref
//! reset). So this layer reads and writes plain dirs and is unit-tested on
//! tempfiles like [`crate::change`].
//!
//! The path's SHAPE decides the semantics (§6) — there is NO object enum and no
//! merge-vs-replace logic:
//!
//! - **Folder path = MIRROR.** The destination becomes byte-identical to the
//!   source; entries the source lacks are DELETED (rsync `--delete`, NOT `cp`).
//!   This is how a close (a file deletion) PROPAGATES through `install tasks`
//!   — the resurrection problem dissolves by addressing, no tombstone needed.
//! - **File / glob path = UNION.** Each source file is copied in, source wins on
//!   overlap, the destination's other files are untouched. No conflict detection;
//!   `install tasks/*` unions, `install tasks/bl-1234.md` ports one — git is the
//!   recovery net.
//!
//! Siblings are never touched: install only ever writes under `<path>`, so
//! `install config` can never eat a co-resident `tasks/` (§2) — forbidden, not
//! merely discouraged. More-specific paths are less destructive.
//!
//! `bin/` never travels: `config/plugins/bin/<name>` is gitignored local state
//! (§2), [`BIN`]-excluded from every copy and never removed by a mirror — the
//! recipient resolves its own binaries. After a copy lands on the local landing,
//! [`referenced`] reads the resulting schedule for the [`resolve_and_bind`]
//! worklist.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io;
use std::path::Path;

use crate::hooks::Hooks;
use crate::plugin::describe;
use crate::registry::Registry;

// The git-free path-copy primitives live in a sibling; this layer stays the
// path-shape dispatch ([`install`]) + the plugin-binding half.
#[path = "install_copy.rs"]
mod copy;

/// `config/plugins/bin` — local, gitignored binary symlinks (§2). install never
/// copies it (committed-tree-only) and a mirror never deletes it; the recipient
/// resolves its own binaries ([`resolve_and_bind`]).
const BIN: &str = "config/plugins/bin";

/// The recommended bundle when `<path>` is omitted: all of `config/`, never the
/// store (§6). `tasks/` is a top-level SIBLING of `config/` (§2), so mirroring
/// `config` excludes it for free — no special case.
pub const DEFAULT_PATH: &str = "config";

/// Which plugins the landing's schedule references, mapped to the op tokens they
/// are wired into — the [`resolve_and_bind`] worklist a `--to`-local install runs.
pub type Referenced = BTreeMap<String, BTreeSet<String>>;

/// What a path-copy changed. install PRINTS this on stdout so the blast radius is
/// visible before you trust it (§6); the commit is the undo, `git diff` the review.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Source files copied into the destination (new or overwritten).
    pub added: usize,
    /// Destination files a mirror removed (always 0 for a union).
    pub deleted: usize,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} added / {} deleted", self.added, self.deleted)
    }
}

/// Why an install could not link a plugin.
#[derive(Debug)]
pub enum InstallError {
    /// The resolved binary does not declare an op it is wired into, or does not
    /// speak balls' protocol version — install refuses to link it (§6).
    Unsupported { name: String, reason: String },
    /// An underlying filesystem or self-describe failure.
    Io(io::Error),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

/// Copy `path` (relative to the checkout root) from `from` into `to`. The path's
/// SHAPE decides: a directory MIRRORS (deletions propagate), a file or `*`-glob
/// UNIONS (additive, source-wins). `bin/` is never touched. Returns the change
/// [`Summary`]. Non-destructive to `from`: this layer only ever reads the source.
pub fn install(path: &str, from: &Path, to: &Path) -> io::Result<Summary> {
    if path.contains('*') {
        return copy::union_glob(path, from, to);
    }
    let src = from.join(path);
    let dst = to.join(path);
    if src.is_dir() || (!src.exists() && dst.is_dir()) {
        copy::mirror(&src, &dst, &from.join(BIN), &to.join(BIN))
    } else {
        copy::union_file(&src, &dst)
    }
}

/// The bind worklist after an install: which plugins the landing's resulting
/// `config/plugins.toml` schedule references (name → ops), each a
/// [`resolve_and_bind`] target. An absent schedule references nothing.
pub fn referenced(landing: &Path) -> io::Result<Referenced> {
    let toml = landing.join("config").join("plugins.toml");
    if toml.is_file() {
        return Ok(Hooks::load_from(&toml)?.referenced());
    }
    Ok(Referenced::new())
}

/// Resolve `name` to the local `candidate` binary and bind it (§6 two-level
/// stitch). Refuses unless the binary's `<bin> protocol` self-description declares
/// every `op` the wiring uses it for and speaks `protocol`. The binary edge
/// supplies `candidate` (a PATH lookup or an explicit `--bin name=path`), keeping
/// this layer env-free.
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

/// The §8 run-wiring (`bl install` end to end) lives in a sibling module; this
/// layer stays the pure, git-free path-copy it is unit-tested as.
#[path = "install_run.rs"]
mod wiring;
pub use wiring::run;
pub(crate) use wiring::{bind_referenced, seal_copy, Chain};

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
