//! §12/§13 checkout-lifecycle ops — `bl prime` and `bl sync`, wired to the
//! engine. These author no ball-file diff, so they run the DIFFLESS shape (§8
//! "skip steps 1/3/5"): no change worktree, no seal — the configured plugin
//! chain runs against `operating/` directly ([`crate::lifecycle::Engine`]).
//!
//! - **`prime`** is the idempotent orchestrator (§12): bootstrap the landing on
//!   a miss ([`crate::substrate`]), set the trail pointer (`--center`/`--stealth`,
//!   the pointer is prime's alone), then run the `prime` chain whose `tracker`
//!   handler adopts/founds/stealth-locks the remote. Re-running converges.
//! - **`sync`** is the synchronization primitive (§13): walk the trail to its
//!   terminus ([`crate::trail`]) and run the `sync` chain there (the tracker's
//!   `sync/pre` does the fetch + ff-only). `bl sync <branch>` syncs a named
//!   branch; `bl sync landing` is a no-op — the landing is never a target.
//!
//! Core stays local-only (§0): it walks LOCAL checkouts and commits config; the
//! one component that talks to a remote is the `tracker` plugin the chain runs.

use crate::edge::Edge;
use crate::git;
use crate::layout::CloneDir;
use crate::lifecycle::Engine;
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::tracker::pointer;
use crate::verb::Verb;
use crate::wire::{Binding, OpContext};
use crate::{substrate, trail, STATE_BRANCH};
use std::io;
use std::path::{Path, PathBuf};

/// `bl prime [--as ID] [--center URL | --stealth]` — bring this checkout to
/// readiness (§12). Bootstrap-on-miss founds the landing; `--center` extends the
/// trail, `--stealth` truncates it; then the `prime` chain runs.
pub fn prime(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse_prime(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let operating = clone.operating();

    if is_landing(&operating) {
        rebind_tracker(&operating, edge)?;
    } else {
        substrate::found(&operating, edge.tracker_bin.as_deref())?;
    }
    if let Some(url) = &opts.center {
        pointer::write(&operating, url)?;
        commit_config(&operating, "balls: set trail pointer")?;
    } else if opts.stealth {
        pointer::clear(&operating)?;
        commit_config(&operating, "balls: truncate trail to stealth")?;
    }

    let remote = remote_for_prime(&operating, &clone, &opts);
    let binding = binding(&operating, &edge.invocation_path, remote, STATE_BRANCH);
    run_chain(&clone, Verb::Prime, &opts.actor, edge.depth, binding)
}

/// `bl sync [BRANCH] [--as ID]` — make state consistent (§13). With no branch
/// (or `terminus`), walk the trail and sync its terminus task store; a named
/// branch syncs that branch on this checkout; `landing` is a no-op.
pub fn sync(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse_sync(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let operating = clone.operating();
    if !is_landing(&operating) {
        return Err(io::Error::other("no balls checkout here — run `bl prime` first"));
    }
    // §13: the landing is never a sync TARGET (no upstream, no task state), so
    // `bl sync landing` is a no-op — for free, not by special case.
    if opts.branch.as_deref() == Some("landing") {
        return Ok(());
    }
    let (terminus, branch) = sync_target(&operating, &opts);
    let remote = origin_of(&terminus);
    let binding = binding(&terminus, &edge.invocation_path, remote, &branch);
    run_chain(&clone, Verb::Sync, &opts.actor, edge.depth, binding)
}

/// The checkout + branch a `sync` targets. A named branch (other than the
/// `terminus` alias) syncs that branch on this checkout; otherwise walk the
/// trail and take its terminus (last node) — the task store.
fn sync_target(operating: &Path, opts: &SyncOpts) -> (PathBuf, String) {
    match opts.branch.as_deref() {
        Some(b) if b != "terminus" => (operating.to_path_buf(), b.to_string()),
        _ => {
            let trail = trail::walk(operating.to_path_buf(), &mut |c| next_local(c));
            (trail.into_iter().last().expect("a trail is never empty"), STATE_BRANCH.to_string())
        }
    }
}

/// Resolve `cur`'s next trail hop to a LOCAL checkout, or `None` at the trail's
/// end. From core's seat every checkout is its own terminus today: materializing
/// a remote `next:` into a local hop is the tracker's job (§12 SEAM, follow-up
/// bl-02c3). This is the seam — when the tracker materializes hops, this resolves
/// them and [`trail::walk`] traverses them with no change here.
fn next_local(_cur: &Path) -> Option<PathBuf> {
    None
}

/// The wire `remote` for `prime`. `--center` drives via the pointer (the tracker
/// resolves it over any wire remote), `--stealth` forces no remote, and a prior
/// stealth lock blocks auto-extension (notice W1, §12). Otherwise auto-discover
/// `origin` — the default landing→origin hop.
fn remote_for_prime(operating: &Path, clone: &CloneDir, opts: &PrimeOpts) -> Option<String> {
    if opts.center.is_some() || opts.stealth {
        return None;
    }
    if clone.root().join("stealth.lock").is_file() {
        eprintln!("bl prime: W1 [tracker] landing locked to stealth, not auto-extending");
        return None;
    }
    origin_of(operating)
}

/// The auto-discovered wire remote — `git remote get-url origin`, a LOCAL config
/// read (no network). Absent origin (the common stealth case) ⇒ `None`.
fn origin_of(operating: &Path) -> Option<String> {
    match git::run(operating, &["remote", "get-url", "origin"], None) {
        Ok(url) => Some(url.trim().to_string()),
        Err(_) => None,
    }
}

/// Run the DIFFLESS chain for `op` against the clone's `operating/` (§13). The
/// terminus seam is unused (diffless never seals) but the engine takes one.
fn run_chain(clone: &CloneDir, op: Verb, actor: &str, depth: u32, binding: Binding) -> io::Result<()> {
    let operating = clone.operating();
    let reg = Registry::at(&operating);
    let pre = reg.resolve(op.token(), "pre")?;
    let post = reg.resolve(op.token(), "post")?;
    let ctx = OpContext::diffless(actor.to_string(), binding);
    let plugins = Subprocess::new(ctx, &clone.root().join("logs"), depth);
    let terminus = git::Git::at(&operating);
    Engine::new(&terminus, &plugins)
        .diffless(op, &operating, &pre, &post)
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Is `operating` already a landing? A founded checkout has a seeded `config/`
/// (§12); `is_dir` follows the symlink a tracked `operating/` is.
fn is_landing(operating: &Path) -> bool {
    operating.join("config").is_dir()
}

/// Re-bind the local `bin/tracker` (idempotent) on an established checkout, so a
/// new session re-derives the machine-local link the committed wiring points at.
fn rebind_tracker(operating: &Path, edge: &Edge) -> io::Result<()> {
    if let Some(bin) = &edge.tracker_bin {
        Registry::at(operating).bind("tracker", bin)?;
    }
    Ok(())
}

/// Stage everything and commit `message` as config (§12: the pointer is
/// committed + portable). Idempotent — an unchanged tree commits nothing, so a
/// re-`prime` with the same pointer converges to a no-op.
fn commit_config(operating: &Path, message: &str) -> io::Result<()> {
    git::run(operating, &["add", "-A"], None)?;
    if !git::run(operating, &["status", "--porcelain"], None)?.trim().is_empty() {
        git::run(operating, &["commit", "-q", "-m", message], None)?;
    }
    Ok(())
}

/// Build the §7 binding for a diffless op over `operating` on `branch`.
fn binding(operating: &Path, invocation: &Path, remote: Option<String>, branch: &str) -> Binding {
    Binding {
        remote,
        branch: branch.to_string(),
        operating: operating.to_string_lossy().into_owned(),
        invocation_path: invocation.to_string_lossy().into_owned(),
    }
}

/// Parsed `bl prime` flags. `--center` and `--stealth` are mutually exclusive.
struct PrimeOpts {
    actor: String,
    center: Option<String>,
    stealth: bool,
}

/// Parsed `bl sync` flags: an optional positional branch + `--as`.
struct SyncOpts {
    actor: String,
    branch: Option<String>,
}

fn parse_prime(args: &[String], default_actor: &str) -> io::Result<PrimeOpts> {
    let mut o = PrimeOpts { actor: default_actor.to_string(), center: None, stealth: false };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            "--center" => o.center = Some(value(args, &mut i, "--center")?),
            "--stealth" => o.stealth = true,
            other => return Err(io::Error::other(format!("prime: unexpected argument '{other}'"))),
        }
        i += 1;
    }
    if o.center.is_some() && o.stealth {
        return Err(io::Error::other("prime: --center and --stealth are mutually exclusive"));
    }
    Ok(o)
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
/// error (the shared parse step for `--as`/`--center`).
fn value(args: &[String], i: &mut usize, flag: &str) -> io::Result<String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| io::Error::other(format!("{flag} needs a value")))
}

#[cfg(test)]
#[path = "checkout_tests.rs"]
mod tests;
