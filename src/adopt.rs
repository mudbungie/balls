//! §6/§13 `prime --install CENTER` config adoption — copy a center's committed
//! `config/` into this landing, with the remote fetch done by the TRACKER, never
//! core.
//!
//! The invariant (§0): core talks to no remote — the tracker plugin is the only
//! remote-talker. So adoption splits cleanly in two:
//!
//! 1. [`fetch_config`] runs the `install.pre` hook chain. The tracker's
//!    `install/pre` handler fetches the center's `balls/config` branch into the
//!    LANDING repo, leaving it at `FETCH_HEAD` (a git-standard ref — no invented
//!    core↔plugin convention). This is the ONE remote read; it rides a plugin.
//! 2. [`install_local`] is pure LOCAL git: materialize that `FETCH_HEAD` in a
//!    throwaway detached worktree, path-copy `config/` into the landing
//!    ([`crate::install`], folder = mirror — config is single-owner,
//!    install-replaced, so adoption is destructive, §2/§6), validate-and-bind the
//!    now-referenced plugins to this box's binaries, and commit.
//!
//! Config "crosses into a landing only by the explicit copy `install` performs"
//! (§0); here that copy is local and the read that feeds it is the tracker's. The
//! ADOPT direction always seals to the local landing (unambiguous), so this needs
//! no resolution of `install --to <center>`'s remote-seal target (bl-66e7). The
//! surrounding [`crate::checkout::prime`] then drives prime+sync against the
//! just-adopted `tasks_branch` — a SINGLE hop, no recursion (a center's config
//! names its own store, never another config, §4). Idempotent so a failed adopt
//! RESUMES: the worktree is force-recreated, the mirror re-copies identical
//! bytes, and an unchanged tree skips the commit.

use crate::checkout;
use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::git;
use crate::hooks::Hooks;
use crate::install;
use crate::lifecycle::Plugins;
use crate::log::{self, Level, Log};
use crate::message::PROTOCOL;
use crate::op::Phase;
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::verb::Verb;
use crate::wire::OpContext;
use crate::LANDING_BRANCH;
use std::io;
use std::path::Path;

/// Adopt `center`'s committed `config/` into `landing` (§6/§13): the tracker
/// fetches (remote), then core copies (local). `store` is the sibling store
/// checkout the §7 binding names; `actor` rides the wire.
pub fn adopt(edge: &Edge, landing: &Path, store: &Path, actor: &str, center: &str) -> io::Result<()> {
    fetch_config(edge, landing, store, actor, center)?;
    install_local(edge, landing)
}

/// Run the `install.pre` chain so the tracker fetches `center`'s `balls/config`
/// into the landing repo (§13). Builds the §7 binding with `center` as the remote
/// and the config branch as the target, then invokes each `install.pre` plugin
/// with cwd = the landing (where the fetch — and core's later materialize — both
/// read `FETCH_HEAD`). An EMPTY chain is a hard error: `prime --install` cannot
/// reach a remote without a fetch plugin (the tracker), so adopting is impossible
/// rather than silently producing nothing — the auto-safe degrade is *plain*
/// prime, not `--install`.
fn fetch_config(edge: &Edge, landing: &Path, store: &Path, actor: &str, center: &str) -> io::Result<()> {
    let cfg = EffectiveConfig::resolve(landing, &edge.xdg.user_config())?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level));
    let binding = checkout::binding(landing, store, &edge.invocation_path, Some(center.to_string()), LANDING_BRANCH.to_string());
    let pre = Hooks::effective(landing, &edge.xdg.user_config())?
        .resolve(&Registry::at(landing), Verb::Install.token(), "pre");
    if pre.is_empty() {
        return Err(io::Error::other(
            "prime --install: no install.pre plugin (e.g. the tracker) is installed to fetch the center's config",
        ));
    }
    let log = Log::new(edge.xdg.clone_dir(&edge.invocation_path).op_log(), level, Verb::Install, log::wall);
    let plugins = Subprocess::new(OpContext::diffless(actor.to_string(), binding), &log, edge.depth);
    for plugin in &pre {
        plugins.run(plugin, Verb::Install, Phase::Pre, landing, None)?;
    }
    Ok(())
}

/// Materialize the fetched config (`FETCH_HEAD`, left by [`fetch_config`]) and
/// copy it into the landing — pure LOCAL git. A throwaway detached worktree
/// (force-recreated so a resumed adopt never trips on a stale one) gives
/// [`install::install`] a source root; `config/` mirrors in, the worktree is torn
/// down, the referenced plugins bind, and the change commits.
pub(crate) fn install_local(edge: &Edge, landing: &Path) -> io::Result<()> {
    let src = edge.xdg.clone_dir(&edge.invocation_path).change("install");
    let src_str = src.to_string_lossy().into_owned();
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
