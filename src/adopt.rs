//! §6/§13 `prime --install CENTER` config adoption — the consent-gated copy of a
//! center's committed `config/` into this landing.
//!
//! This is the §2 config **install-transport**: config "crosses into a landing
//! only by the explicit copy `install` performs" (§0), so adopting a center's
//! config is the ONE sanctioned remote READ in core (a fetch, never a push),
//! distinct from the tracker's store sync/push. Config is single-owner and
//! install-replaced, so the copy is DESTRUCTIVE (folder = MIRROR, §6), never a
//! merge. The flow [`adopt`] runs is the install half of the §13 fuse: fetch the
//! center's landing branch, MATERIALIZE it in a throwaway detached worktree (a
//! LOCAL git act on the just-fetched ref), path-copy `config/` in
//! ([`crate::install`]), validate-and-bind the now-referenced plugins to this
//! box's binaries, and commit. The surrounding [`crate::checkout::prime`] then
//! drives prime+sync against the just-adopted `tasks_branch` — a SINGLE hop, no
//! recursion (a center's config names its own store, never another config, §4).

use crate::edge::Edge;
use crate::git;
use crate::install;
use crate::message::PROTOCOL;
use crate::registry::Registry;
use crate::LANDING_BRANCH;
use std::io;
use std::path::Path;

/// Adopt `center`'s committed `config/` into `landing` (§6/§13). Idempotent so a
/// failed adopt RESUMES rather than double-applies: the materialization worktree
/// is force-recreated, the mirror re-copies identical bytes, and an unchanged
/// tree skips the commit ([`commit_landing`]). The center is fetched into the
/// landing repo (FETCH_HEAD), checked out detached in the clone's `changes/`
/// area, mirrored in, then torn down. PRINTS the install change summary (§6 —
/// the blast radius visible before you trust it).
pub fn adopt(edge: &Edge, landing: &Path, center: &str) -> io::Result<()> {
    let src = edge.xdg.clone_dir(&edge.invocation_path).change("install");
    let src_str = src.to_string_lossy().into_owned();
    git::run(landing, &["fetch", center, LANDING_BRANCH], None)?;
    let _ = git::run(landing, &["worktree", "remove", "--force", &src_str], None);
    git::run(landing, &["worktree", "add", "--detach", &src_str, "FETCH_HEAD"], None)?;
    let summary = install::install(install::DEFAULT_PATH, &src, landing)?;
    git::run(landing, &["worktree", "remove", "--force", &src_str], None)?;
    bind_referenced(landing, edge.exe_dir.as_deref())?;
    commit_landing(landing)?;
    println!("install: {summary}");
    Ok(())
}

/// Bind every plugin the freshly-adopted `config/plugins.toml` references to this
/// machine's sibling binary beside `bl` (`exe_dir`), validating each against its
/// live `<bin> protocol` self-description before linking (§6
/// [`install::resolve_and_bind`] — refuses an op or protocol version the binary
/// does not declare). A referenced name with no sibling here stays dangling — the
/// clean "referenced but not installed" dispatch error (§6), never bound
/// silently. `exe_dir == None` ⇒ a plugin-free box (nothing to bind).
fn bind_referenced(landing: &Path, exe_dir: Option<&Path>) -> io::Result<()> {
    let registry = Registry::at(landing);
    for (name, ops) in install::referenced(landing)? {
        if let Some(bin) = exe_dir.map(|d| d.join(&name)).filter(|p| p.exists()) {
            install::resolve_and_bind(&registry, &name, &bin, &ops, PROTOCOL)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
    }
    Ok(())
}

/// Commit the adopted `config/` onto the landing (§6 install is committed —
/// `git diff` reviews it, the commit is the undo). Stage everything, then commit
/// ONLY when something is staged: a re-adopt of identical config leaves an empty
/// index (`diff --cached --quiet` succeeds) and skips the commit, so the verb
/// converges to a no-op (§13 idempotence).
fn commit_landing(landing: &Path) -> io::Result<()> {
    git::run(landing, &["add", "-A"], None)?;
    if git::run(landing, &["diff", "--cached", "--quiet"], None).is_err() {
        git::run(landing, &["commit", "-q", "-m", "balls: install"], None)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "adopt_tests.rs"]
mod tests;
