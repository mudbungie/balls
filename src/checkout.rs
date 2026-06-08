//! §12/§13 checkout-lifecycle ops — `bl prime` and `bl sync`, wired to the
//! engine. These author no ball-file diff, so they run the DIFFLESS shape (§8
//! "skip steps 1/3/5"): no change worktree, no seal — the configured plugin
//! chain runs against the STORE checkout directly ([`crate::lifecycle::Engine`]).
//!
//! - **`prime`** is the idempotent orchestrator of syncs (§12/§13): bootstrap
//!   BOTH branches on a miss ([`crate::substrate`] — the `balls/config` landing +
//!   the `tasks_branch` store), run the `prime` chain whose `tracker` handler
//!   adopts/founds/stealth-locks the remote, THEN drive `sync` against the store
//!   so an established checkout is brought current. It gets currency by invoking
//!   the sync primitive, never a reimplemented fetch (the single-codepath
//!   invariant). Re-running converges.
//! - **`sync`** is the synchronization primitive (§13): run the `sync` chain
//!   against the store (the tracker's `sync/pre` does the fetch + ff-only). With
//!   no arg it syncs the config `tasks_branch`; `bl sync <branch>` PULLS that
//!   named branch (the positional substitutes `tasks_branch` in the binding).
//!   `bl sync landing` is a no-op — the landing is never a sync target (§13).
//!
//! Core stays local-only (§0): it ensures the two LOCAL checkouts and reads
//! config from the landing; the one component that talks to a remote is the
//! `tracker` plugin the chain runs. The §7 binding it builds is the ONE
//! construction point ([`binding`]) shared with [`crate::mutate`].

use crate::adopt;
use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::git;
use crate::hooks::Hooks;
use crate::lifecycle::Engine;
use crate::log::{self, Level, Log};
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::seed;
use crate::substrate;
use crate::verb::Verb;
use crate::wire::{Binding, OpContext};
use std::io;
use std::path::Path;

/// `bl prime [--as ID] [--install CENTER]` — bring this checkout to readiness
/// (§12/§13). Bootstrap-on-miss founds both branches; the `prime` chain
/// (adopt/found) runs against the store; THEN prime drives `sync` so an
/// established checkout is brought current. Prime's binding already names the
/// config `tasks_branch` with the resolved remote — exactly what a no-arg `sync`
/// binds — so the SAME binding serves both: prime calls the `sync` primitive,
/// never a reimplemented fetch. Idempotent: a just-founded remote's sync fetch is
/// a no-op; in stealth the tracker `sync/pre` no-ops.
///
/// `--install CENTER` fuses prime + install + prime on demand (§13): after the
/// substrate exists, [`adopt`] copies the center's committed `config/` into the
/// landing (the consent-gated §6 install — the `--install` flag IS the consent),
/// THEN this same call's prime+sync chains bring the just-adopted `tasks_branch`
/// to readiness. It is a SINGLE hop, not a walk: a center's config names its own
/// `tasks_branch` (the one config→store indirection, §4), never another config to
/// chase. The center also seeds the store remote (top of the [`resolve_remote`]
/// precedence) unless an explicit `--remote` overrides it. Plain prime (no
/// `--install`) never adopts foreign config nor activates code — the auto-safe
/// every-session path holds.
pub fn prime(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse_prime(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());

    if is_landing(&landing) {
        seed::rebind(&landing, edge.exe_dir.as_deref())?;
    } else {
        substrate::found(&landing, &store, &edge.xdg, edge.exe_dir.as_deref())?;
    }
    if let Some(center) = &opts.install {
        adopt::adopt(edge, &landing, center)?;
    }
    let remote = opts.remote.or(opts.install);
    let (binding, level) = bind(edge, &landing, &store, remote, None)?;
    run_chain(edge, &landing, &store, Verb::Prime, &opts.actor, binding.clone(), level)?;
    run_chain(edge, &landing, &store, Verb::Sync, &opts.actor, binding, level)
}

/// `bl sync [BRANCH] [--as ID]` — make state consistent (§13): run the `sync`
/// chain against the store (the tracker's `sync/pre` fetches + ff-only). With no
/// arg it syncs the config-named `tasks_branch`; `bl sync <branch>` PULLS that
/// named branch instead — the positional substitutes `tasks_branch` in the §7
/// binding, the one datum the tracker fetches/ff's against. A `landing` target
/// is a no-op — the landing is never a sync target (§2/§13).
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
    let (binding, level) = bind(edge, &landing, &store, None, opts.branch)?;
    run_chain(edge, &landing, &store, Verb::Sync, &opts.actor, binding, level)
}

/// [`Level`]: the §12-resolved store remote, the `tasks_branch` (the `target`
/// override, else the config-named one — §13 `bl sync <branch>`), the two checkout
/// paths, and the `log_level` threshold (CLI override over config). `cli_remote` is
/// the parsed `--remote`/`--center` override (prime; `None` for sync/mutate),
/// seeding the [`resolve_remote`] precedence. One landing config read serves both.
/// The single construction point [`binding`] does the binding assembly.
fn bind(edge: &Edge, landing: &Path, store: &Path, cli_remote: Option<String>, target: Option<String>) -> io::Result<(Binding, Level)> {
    let user_config = edge.xdg.user_config();
    let cfg = EffectiveConfig::resolve(landing, &user_config)?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level));
    let remote = resolve_remote(cli_remote, landing, &user_config);
    let tasks_branch = target.unwrap_or(cfg.tasks_branch);
    let binding = binding(landing, store, &edge.invocation_path, remote, tasks_branch);
    Ok((binding, level))
}

/// Resolve the store remote by the §12 precedence — an explicit CLI override
/// (`--remote` > `--center`, already collapsed by the caller) beats the
/// per-machine XDG `remote`, which beats auto-discovered `origin`. `None` ⇒ no
/// remote resolved = stealth (the store stays local). Shared by every op's bind
/// (prime/sync/mutate) so they agree on ONE upstream for `tasks_branch`.
pub(crate) fn resolve_remote(cli: Option<String>, landing: &Path, user_config: &Path) -> Option<String> {
    cli.or_else(|| crate::config::xdg_remote(user_config))
        .or_else(|| origin_of(landing))
}

/// The auto-discovered store remote — `git remote get-url origin` on the landing,
/// a LOCAL config read (no network). Absent origin (the common stealth case) ⇒
/// `None`. The bottom of the [`resolve_remote`] precedence.
fn origin_of(checkout: &Path) -> Option<String> {
    match git::run(checkout, &["remote", "get-url", "origin"], None) {
        Ok(url) => Some(url.trim().to_string()),
        Err(_) => None,
    }
}

/// Run the DIFFLESS chain for `op` (§13): resolve the plugin sets from the
/// LANDING's `config/plugins.toml` `[hooks]` schedule (§6), then run them with
/// cwd = the STORE checkout and the anvil bracketing the store-branch HEAD.
fn run_chain(edge: &Edge, landing: &Path, store: &Path, op: Verb, actor: &str, binding: Binding, level: Level) -> io::Result<()> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let hooks = Hooks::load(landing)?;
    let reg = Registry::at(landing);
    let pre = hooks.resolve(&reg, op.token(), "pre");
    let post = hooks.resolve(&reg, op.token(), "post");
    let ctx = OpContext::diffless(actor.to_string(), binding);
    let log = Log::new(clone.op_log(), level, op, log::wall);
    let plugins = Subprocess::new(ctx, &log, edge.depth);
    let anvil = git::Git::at(store);
    Engine::new(&anvil, &plugins, &log)
        .diffless(op, store, &pre, &post)
        .map_err(|e| io::Error::other(e.to_string()))
}

/// Is the landing already founded? A founded checkout has a seeded `config/`
/// folder (§12) in its working tree.
fn is_landing(landing: &Path) -> bool {
    landing.join("config").is_dir()
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

/// Parsed `bl prime` flags: the resolved actor, the optional store-remote
/// override that seeds the top of the [`resolve_remote`] precedence (§12), and the
/// optional `--install CENTER` that triggers config adoption (§13). `install`
/// also seeds the remote when `remote` is unset (the center is where the adopted
/// `tasks_branch` lives), resolved in [`prime`].
struct PrimeOpts {
    actor: String,
    remote: Option<String>,
    install: Option<String>,
}

/// Parse `bl prime [--as ID] [--remote URL] [--center URL] [--install CENTER]`.
/// `--remote` and `--center` both name the store remote (the federation framing
/// differs, the effect is one URL); `--remote` wins if both are given, whatever
/// the order (`get_or_insert` lets a later `--center` fill an empty slot but never
/// overwrite a `--remote`, which always assigns). `--install` names the center to
/// adopt config from (§13). An unknown flag or positional is an error.
fn parse_prime(args: &[String], default_actor: &str) -> io::Result<PrimeOpts> {
    let mut o = PrimeOpts { actor: default_actor.to_string(), remote: None, install: None };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            "--remote" => o.remote = Some(value(args, &mut i, "--remote")?),
            "--center" => {
                let center = value(args, &mut i, "--center")?;
                o.remote.get_or_insert(center);
            }
            "--install" => o.install = Some(value(args, &mut i, "--install")?),
            other => return Err(io::Error::other(format!("prime: unexpected argument '{other}'"))),
        }
        i += 1;
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
