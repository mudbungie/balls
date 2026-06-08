//! §12/§13 checkout-lifecycle ops — `bl prime` and `bl sync`, wired to the
//! engine. These author no ball-file diff, so they run the DIFFLESS shape (§8
//! "skip steps 1/3/5"): no change worktree, no seal — the configured plugin
//! chain runs against the STORE checkout directly ([`crate::lifecycle::Engine`]).
//!
//! - **`prime`** is the idempotent orchestrator (§12): bootstrap BOTH branches on
//!   a miss ([`crate::substrate`] — the `balls/config` landing + the `tasks_branch`
//!   store), then run the `prime` chain whose `tracker` handler adopts/founds/
//!   stealth-locks the remote. Re-running converges.
//! - **`sync`** is the synchronization primitive (§13): run the `sync` chain
//!   against the store (the tracker's `sync/pre` does the fetch + ff-only).
//!   `bl sync landing` is a no-op — the landing is never a sync target (§13).
//!
//! Core stays local-only (§0): it ensures the two LOCAL checkouts and reads
//! config from the landing; the one component that talks to a remote is the
//! `tracker` plugin the chain runs. The §7 binding it builds is the ONE
//! construction point ([`binding`]) shared with [`crate::mutate`].

use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::git;
use crate::lifecycle::Engine;
use crate::log::{self, Level, Log};
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::substrate;
use crate::verb::Verb;
use crate::wire::{Binding, OpContext};
use std::io;
use std::path::Path;

/// `bl prime [--as ID]` — bring this checkout to readiness (§12). Bootstrap-on-
/// miss founds both branches; then the `prime` chain runs against the store.
pub fn prime(edge: &Edge, args: &[String]) -> io::Result<()> {
    let actor = parse(args, &edge.default_actor, "prime")?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());

    if is_landing(&landing) {
        rebind_tracker(&landing, edge)?;
    } else {
        substrate::found(&landing, &store, edge.tracker_bin.as_deref())?;
    }
    let (binding, level) = bind(edge, &landing, &store)?;
    run_chain(edge, &landing, &store, Verb::Prime, &actor, binding, level)
}

/// `bl sync [BRANCH] [--as ID]` — make state consistent (§13): run the `sync`
/// chain against the store (the tracker's `sync/pre` fetches + ff-only). A
/// `landing` target is a no-op — the landing is never a sync target (§2/§13).
pub fn sync(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse_sync(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    if !is_landing(&landing) {
        return Err(io::Error::other("no balls checkout here — run `bl prime` first"));
    }
    if opts.branch.as_deref() == Some("landing") {
        return Ok(());
    }
    let (binding, level) = bind(edge, &landing, &store)?;
    run_chain(edge, &landing, &store, Verb::Sync, &opts.actor, binding, level)
}

/// Build the §7 binding for a checkout-lifecycle op plus the resolved §4 log
/// [`Level`]: the auto-discovered store remote, the config-named `tasks_branch`,
/// the two checkout paths, and the `log_level` threshold (CLI override over
/// config). One landing config read serves both. The single construction point
/// [`binding`] does the binding assembly.
fn bind(edge: &Edge, landing: &Path, store: &Path) -> io::Result<(Binding, Level)> {
    let cfg = EffectiveConfig::resolve(landing, &edge.xdg.user_config())?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level));
    let binding = binding(landing, store, &edge.invocation_path, origin_of(landing), cfg.tasks_branch);
    Ok((binding, level))
}

/// The auto-discovered store remote — `git remote get-url origin` on the landing,
/// a LOCAL config read (no network). Absent origin (the common stealth case) ⇒
/// `None`. Shared with [`crate::mutate`], the mutating-verb dispatch that resolves
/// the same §7 binding for a ball-file op.
pub(crate) fn origin_of(checkout: &Path) -> Option<String> {
    match git::run(checkout, &["remote", "get-url", "origin"], None) {
        Ok(url) => Some(url.trim().to_string()),
        Err(_) => None,
    }
}

/// Run the DIFFLESS chain for `op` (§13): resolve the `NN-` plugin sets from the
/// LANDING registry (the chain lives on `config/plugins`, §2), then run them with
/// cwd = the STORE checkout and the terminus bracketing the store-branch HEAD.
fn run_chain(edge: &Edge, landing: &Path, store: &Path, op: Verb, actor: &str, binding: Binding, level: Level) -> io::Result<()> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let reg = Registry::at(landing);
    let pre = reg.resolve(op.token(), "pre")?;
    let post = reg.resolve(op.token(), "post")?;
    let ctx = OpContext::diffless(actor.to_string(), binding);
    let log = Log::new(clone.op_log(), level, op, log::wall);
    let plugins = Subprocess::new(ctx, &log, edge.depth);
    let terminus = git::Git::at(store);
    Engine::new(&terminus, &plugins, &log)
        .diffless(op, store, &pre, &post)
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Is the landing already founded? A founded checkout has a seeded `config/`
/// folder (§12) in its working tree.
fn is_landing(landing: &Path) -> bool {
    landing.join("config").is_dir()
}

/// Re-bind the local `bin/tracker` (idempotent) on an established checkout, so a
/// new session re-derives the machine-local link the committed wiring points at.
fn rebind_tracker(landing: &Path, edge: &Edge) -> io::Result<()> {
    if let Some(bin) = &edge.tracker_bin {
        Registry::at(landing).bind("tracker", bin)?;
    }
    Ok(())
}

/// Build the §7 binding for an op over the two checkouts (§7). Shared with
/// [`crate::mutate`] — a mutating ball-file op binds the same way a diffless
/// checkout op does. The ONE construction point for the new binding shape.
pub(crate) fn binding(landing: &Path, store: &Path, invocation: &Path, remote: Option<String>, tasks_branch: String) -> Binding {
    Binding {
        remote,
        tasks_branch,
        store: store.to_string_lossy().into_owned(),
        landing: landing.to_string_lossy().into_owned(),
        invocation_path: invocation.to_string_lossy().into_owned(),
    }
}

/// Parsed `bl sync` flags: an optional positional branch + `--as`.
struct SyncOpts {
    actor: String,
    branch: Option<String>,
}

/// Parse `[--as ID]` for a verb that takes no other flags (`prime`), returning
/// the resolved actor. An unknown flag or positional is an error.
fn parse(args: &[String], default_actor: &str, verb: &str) -> io::Result<String> {
    let mut actor = default_actor.to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => actor = value(args, &mut i, "--as")?,
            other => return Err(io::Error::other(format!("{verb}: unexpected argument '{other}'"))),
        }
        i += 1;
    }
    Ok(actor)
}

fn parse_sync(args: &[String], default_actor: &str) -> io::Result<SyncOpts> {
    let mut o = SyncOpts { actor: default_actor.to_string(), branch: None };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            flag if flag.starts_with("--") => {
                return Err(io::Error::other(format!("sync: unexpected flag '{flag}'")));
            }
            _ => {
                if o.branch.replace(args[i].clone()).is_some() {
                    return Err(io::Error::other("sync: at most one branch"));
                }
            }
        }
        i += 1;
    }
    Ok(o)
}

/// The value following a `--flag`, advancing the cursor; missing value is an
/// error (the shared parse step for `--as`).
fn value(args: &[String], i: &mut usize, flag: &str) -> io::Result<String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| io::Error::other(format!("{flag} needs a value")))
}

#[cfg(test)]
#[path = "checkout_tests.rs"]
mod tests;
