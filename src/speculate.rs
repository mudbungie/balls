//! The verdict cache — tree-keyed gate memoization (bl-1263, design
//! docs/design/bl-24e7-speculative-merge-queue.md).
//!
//! The pre-commit gate's verdict is a pure function of two content-addressed
//! inputs: the TREE it tests (the worktree exactly as `git add -A` would stage
//! it) and the GATE that tests it (the toolchain plus the gate scripts). So a
//! verdict is one file per `(tree, gate)` pair under the `bl-speculate` plugin
//! territory (§1), and "the stated build matches the merge" is inherent in the
//! key rather than checked — trust reduces to whoever may write the territory,
//! which is builder identity. The store home is LOCAL XDG state, not the center
//! store: a verdict is an assertion by a builder on this trust boundary, and
//! publishing it would widen the boundary without widening the trust (settles
//! the design's open question 3).
//!
//! The consumer is `scripts/pre-commit`: consult [`check`] before running the
//! gates (a hit means this exact tree already passed this exact gate — the run
//! would re-execute a known result), [`record`] after a pass. Both sides fail
//! open — no binary, no verdict, any error → the stock gate runs — so deleting
//! the store restores stock behavior exactly (severability). Speculative
//! builders (bl-d0c2) warm the same records ahead of the merge queue.
//!
//! The env-free policy layer: paths and the toolchain string arrive as
//! arguments; the `bl-speculate` binary edge is the one place the environment
//! is read (the [`crate::chore`] convention).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde::{Deserialize, Serialize};

use crate::safegit;

/// One recorded gate outcome for an exact `(tree, gate)` pair. `pass` is the
/// verdict; `builder` names who ran the gate — the trust root, since anything
/// able to write the territory could write the record.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// Whether the gate passed on this tree.
    pub pass: bool,
    /// The identity that ran the gate.
    pub builder: String,
}

/// The files whose content (with the toolchain string) IS the gate identity:
/// change any of them and every stored verdict silently stops matching, which
/// is exactly the invalidation a gate upgrade must cause.
const GATE_FILES: &[&str] = &[
    "scripts/pre-commit",
    "scripts/check-line-lengths.sh",
    "scripts/check-coverage.sh",
    "Makefile",
];

/// Content-address the gate itself: `toolchain` (the edge passes `rustc -V`)
/// concatenated with each [`GATE_FILES`] body, NUL-separated, hashed by git —
/// the content hasher this system already trusts, so no hash dependency.
pub fn gate_fingerprint(root: &Path, scratch: &Path, toolchain: &str) -> io::Result<String> {
    let mut blob = toolchain.as_bytes().to_vec();
    for rel in GATE_FILES {
        blob.push(0);
        blob.extend_from_slice(&fs::read(root.join(rel))?);
    }
    fs::create_dir_all(scratch)?;
    let file = scratch.join("gate-blob");
    fs::write(&file, blob)?;
    let mut cmd = safegit::at(root);
    cmd.arg("hash-object").arg(&file);
    let out = cmd.output()?;
    let _ = fs::remove_file(&file);
    ok_stdout(&out, "git hash-object")
}

/// The tree the gate actually tests: the worktree as `git add -A` would stage
/// it (gitignore respected, uncommitted edits included), written through a
/// throwaway index in `scratch` so the real index is never touched. Behaves
/// identically in linked worktrees — close's main-folded tree included.
pub fn worktree_oid(root: &Path, scratch: &Path) -> io::Result<String> {
    fs::create_dir_all(scratch)?;
    let index = scratch.join("speculate-index");
    let mut add = safegit::at(root);
    add.env("GIT_INDEX_FILE", &index).args(["add", "-A"]);
    let added = add.output()?;
    let mut write = safegit::at(root);
    write.env("GIT_INDEX_FILE", &index).arg("write-tree");
    let written = write.output()?;
    let _ = fs::remove_file(&index);
    ok_stdout(&added, "git add -A").and(ok_stdout(&written, "git write-tree"))
}

/// `<territory>/verdicts/<tree>-<gate>.toml` — one file per verdict. The id is
/// the path: nothing is stored that the key already says.
#[must_use]
pub fn verdict_path(territory: &Path, tree: &str, gate: &str) -> PathBuf {
    territory.join("verdicts").join(format!("{tree}-{gate}.toml"))
}

/// Read the verdict for a `(tree, gate)` pair. Absence is an honest miss, not
/// an error; a corrupt record is an error (a trusted store must not half-work).
pub fn read(territory: &Path, tree: &str, gate: &str) -> io::Result<Option<Verdict>> {
    let body = match fs::read_to_string(verdict_path(territory, tree, gate)) {
        Ok(body) => body,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    toml::from_str(&body).map(Some).map_err(io::Error::other)
}

/// Write the verdict for a `(tree, gate)` pair, creating the store on first
/// use. Last write wins — verdicts for the same key assert the same fact.
pub fn write(territory: &Path, tree: &str, gate: &str, verdict: &Verdict) -> io::Result<()> {
    let dir = territory.join("verdicts");
    fs::create_dir_all(&dir)?;
    let body = toml::to_string(verdict).map_err(io::Error::other)?;
    fs::write(verdict_path(territory, tree, gate), body)
}

/// TRUE iff this exact worktree tree already PASSED this exact gate — the
/// hook's skip condition. A recorded failure is a miss here: only a pass
/// licenses skipping the run.
pub fn check(root: &Path, scratch: &Path, territory: &Path, toolchain: &str) -> io::Result<bool> {
    let tree = worktree_oid(root, scratch)?;
    let gate = gate_fingerprint(root, scratch, toolchain)?;
    Ok(read(territory, &tree, &gate)?.is_some_and(|v| v.pass))
}

/// Record the outcome the caller just observed for the current tree under the
/// current gate — how every completed gate run (real or speculative) warms the
/// cache for whoever folds to this exact tree next.
pub fn record(
    root: &Path,
    scratch: &Path,
    territory: &Path,
    toolchain: &str,
    pass: bool,
    builder: &str,
) -> io::Result<()> {
    let tree = worktree_oid(root, scratch)?;
    let gate = gate_fingerprint(root, scratch, toolchain)?;
    let verdict = Verdict { pass, builder: builder.to_string() };
    write(territory, &tree, &gate, &verdict)
}

/// Success → trimmed stdout; failure → an error carrying git's stderr voice.
fn ok_stdout(out: &Output, what: &str) -> io::Result<String> {
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let voice = String::from_utf8_lossy(&out.stderr);
        Err(io::Error::other(format!("{what}: {}", voice.trim())))
    }
}

#[cfg(test)]
#[path = "speculate_tests.rs"]
mod speculate_tests;
