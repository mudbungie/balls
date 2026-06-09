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
//! 2. [`install_local`] is pure LOCAL git: SEAL the `config/` path-copy from
//!    `FETCH_HEAD` onto the landing through the shared §8 engine spine
//!    ([`crate::install::seal_copy`] — folder = mirror; config is single-owner,
//!    install-replaced, so adoption is destructive, §2/§6), then validate-and-bind
//!    the now-referenced plugins to this box's binaries. The engine's chains run
//!    EMPTY here: the `install.pre` chain already ran in step 1 (the staging
//!    reads the `FETCH_HEAD` it leaves, so the fetch cannot ride the engine's own
//!    `pre`) — that fetch leg is the one piece still outside the §14 trace.
//!
//! Config "crosses into a landing only by the explicit copy `install` performs"
//! (§0); here that copy is local and the read that feeds it is the tracker's. The
//! ADOPT direction always seals to the local landing (unambiguous), so this needs
//! no resolution of `install --to <center>`'s remote-seal target (bl-66e7). The
//! surrounding [`crate::checkout::prime`] then drives prime+sync against the
//! just-adopted `tasks_branch` — a SINGLE hop, no recursion (a center's config
//! names its own store, never another config, §4). Idempotent so a failed adopt
//! RESUMES: the worktrees are force-recreated, the mirror re-copies identical
//! bytes, and an unchanged tree seals to the existing tip (the no-op seal).

use crate::checkout;
use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::hooks::Hooks;
use crate::install;
use crate::lifecycle::Plugins;
use crate::log::{self, Level, Log};
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

/// SEAL the fetched config (`FETCH_HEAD`, left by [`fetch_config`]) onto the
/// landing through the shared §8 spine ([`install::seal_copy`] — the same
/// materialize/copy/seal `bl install` runs, §14 rollback included), then bind
/// the referenced plugins and print the §6 change summary. The chains run
/// empty — the `install.pre` fetch already ran (see the module doc) — so the
/// subprocess seam is constructed but never invoked.
pub(crate) fn install_local(edge: &Edge, landing: &Path) -> io::Result<()> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (binding, level) = checkout::bind(edge, landing, &clone.store(), None, None)?;
    let log = Log::new(clone.op_log(), level, Verb::Install, log::wall);
    let plugins = Subprocess::new(OpContext::diffless(edge.default_actor.clone(), binding), &log, edge.depth);
    let chain = install::Chain { plugins: &plugins, log: &log, pre: Vec::new(), post: Vec::new() };
    let summary = install::seal_copy(&clone, install::DEFAULT_PATH, "FETCH_HEAD", landing, &chain)?;
    install::bind_referenced(landing, edge.exe_dir.as_deref())?;
    println!("install: {summary}");
    Ok(())
}

#[cfg(test)]
#[path = "adopt_tests.rs"]
mod tests;
