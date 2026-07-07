//! §12/§13 checkout-lifecycle ops — `bl prime` and `bl sync`, wired to the
//! engine. These author no ball-file diff, so they run the DIFFLESS shape (§8
//! "skip steps 1/3/5"): no change worktree, no seal — the configured plugin
//! chain runs against the STORE checkout directly ([`crate::lifecycle::Engine`]).
//!
//! - **`prime`** is the idempotent orchestrator of syncs (§12/§13): found the
//!   `balls/config` LANDING on a miss ([`crate::substrate`]), then run ONE
//!   core-owned pass ([`prime_chain`]) — `prime/pre` (the tracker clones an
//!   established remote store in), [`crate::substrate::materialize`] for the
//!   configured `tasks_branch` (laid down LAZILY, no eager orphan to diverge,
//!   bl-0a23; a `pre` that MOVED the name aborts, bl-698d), `prime/post` (the
//!   tracker's fetch-ff + push) — THEN drives `sync` against the store so an
//!   established checkout is brought current. Currency comes from invoking the
//!   sync primitive, never a reimplemented fetch (the single-codepath
//!   invariant). Re-running converges.
//! - **`sync`** is the synchronization primitive (§13): run the `sync` chain
//!   against the store (the tracker's `sync/pre` does the fetch + ff-only). With
//!   no arg it syncs the config `tasks_branch`; `bl sync <branch>` PULLS that
//!   named branch (the positional substitutes `tasks_branch` in the binding).
//!   Syncing the landing is a no-op FOR FREE: the landing is upstream-less by
//!   construction (§4), and the tracker's general rule — fetch the branch's
//!   upstream, if any — yields nothing for it. Core special-cases no name (§13).
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

/// `bl prime [--as ID] [--remote URL] [--center URL] [--install CENTER]
/// [--stealth]` — bring this checkout to readiness
/// (§12/§13). Bootstrap-on-miss founds the LANDING; the [`prime_chain`] pass
/// materializes the store and runs the `prime` chain; THEN prime drives `sync` so
/// an established checkout is brought current. Prime's binding already names the
/// config `tasks_branch` with the resolved remote — exactly what a no-arg `sync`
/// binds — so the SAME binding serves both: prime calls the `sync` primitive,
/// never a reimplemented fetch. Idempotent: a just-founded remote's sync fetch is
/// a no-op; in stealth the tracker `sync/pre` no-ops.
///
/// `--center URL` ENROLLS this satellite into a shared center in ONE command
/// (bl-35e5, §13 Q3): it is prime-only and DURABLE by definition — sugar for the
/// composition that already exists, `bl conf set task-remote URL` (the per-clone
/// binding write, [`crate::conf::bind_task_remote`]) + `--install URL` (config
/// adoption) + ordinary prime. Re-running converges (the binding write converges,
/// install re-copies identical bytes, sync fast-forwards). A filesystem path is a
/// legitimate center — two repos on one box share through a local bare repo, the
/// same code path as a hosted one (§Q4). It SUBSUMES `--install` (both name the
/// center whose config we adopt), so the two are mutually exclusive at parse. The
/// rule: `--remote` shapes one op; `--center` enrolls a checkout.
///
/// `--install CENTER` fuses prime + install + prime on demand (§13): after the
/// substrate exists, [`adopt`] copies the center's committed `config/` into the
/// landing (the consent-gated §6 install — the `--install` flag IS the consent),
/// THEN this same call's prime+sync chains bring the just-adopted `tasks_branch`
/// to readiness. It is a SINGLE hop, not a walk: a center's config names its own
/// `tasks_branch` (the one config→store indirection, §4), never another config to
/// chase. The center also seeds the store remote (the explicit remote used for
/// the binding) unless an explicit `--remote` overrides it — but WITHOUT the
/// durable binding `--center` writes (that is the whole difference: `--install`
/// adopts once, `--center` also enrolls). Plain prime (no `--install`/`--center`)
/// never adopts foreign config nor activates code — the auto-safe every-session
/// path holds.
///
/// `--stealth` is the §12 consent opt-out, and it is DURABLE: sugar for
/// `bl conf set task-remote none` — one committed landing-config write of the
/// stealth sentinel (an explicit flag you typed is the §4 "by you" path), which
/// every later op's [`bind`] derives its stealth from. Consent withheld binds
/// the CHECKOUT, not one prime invocation (bl-9df0). It contradicts
/// `--remote`/`--center`/`--install` (each names a remote), refused at parse.
pub fn prime(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse_prime(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());

    // The founding seed's prune notes (a pruned name with a [source] hint,
    // bl-5b09) are carried forward into the op log once it exists — founding
    // precedes the threshold read, so the seed cannot log them itself. The
    // rebind path prunes nothing and has none.
    let seed_notes = if is_landing(&landing) {
        seed::rebind(&landing, edge.exe_dir.as_deref())?;
        Vec::new()
    } else {
        substrate::found_landing(&landing, &edge.xdg, edge.exe_dir.as_deref(), &opts.actor)?
    };
    if opts.stealth {
        crate::conf::declare_stealth(&landing, &opts.actor)?;
    }
    // `--center` ENROLLS (bl-35e5): write the durable per-clone binding — exactly
    // what `bl conf set task-remote URL` does — BEFORE adopt/prime, so the §12
    // ladder resolves the center on this op AND every later one (enrollment is
    // prime-only but durable by definition; a failed adopt below leaves the
    // binding, so re-running converges). `--center` then SUBSUMES `--install`:
    // both name the center whose committed config we adopt (mutually exclusive at
    // parse, so at most one is set).
    if let Some(center) = &opts.center {
        crate::conf::bind_task_remote(&clone, &landing, &opts.actor, center)?;
    }
    let install = opts.install.as_ref().or(opts.center.as_ref());
    if let Some(center) = install {
        adopt::adopt(edge, &landing, &store, &opts.actor, center)?;
    }
    // `--center`'s remote is now the durable binding (resolved by the ladder, so
    // pass None); `--install` alone carries no durable binding, so it seeds this
    // op's remote directly (the center is where the adopted `tasks_branch` lives).
    let remote = opts.remote.or(opts.install);
    let (binding, level) = bind(edge, &landing, &store, remote, None)?;
    prime_chain(edge, &landing, &store, &opts.actor, binding.clone(), level, &seed_notes)?;
    run_chain(edge, &landing, &store, Verb::Sync, &opts.actor, binding, level)
}

/// Run `prime`'s §12 chain — ONE pass (bl-698d): the `prime/pre` chain (the
/// tracker clones an established remote branch in, or no-ops), then core
/// [`substrate::materialize`]s the store for the configured `tasks_branch`, then
/// the `prime/post` chain (the tracker's fetch-ff + publish) against the
/// now-materialized store. `pre` runs with cwd = the LANDING (the store is not
/// laid down until materialize, so it cannot be the cwd on a first prime);
/// `post` with cwd = the store. The `step` closure is core's between-phase work
/// — materialize, then report whether the configured name MOVED across `pre`. A
/// moved name ABORTS the op in the engine: no conformant plugin rewrites
/// `tasks_branch` (the tracker's name-settle is warn-only; config crosses only
/// by `install`, §12), so the check ENFORCES the consent rule instead of looping
/// to accommodate violations of it (supersedes the bl-0a23 fixpoint and its
/// bl-33db pass cap).
fn prime_chain(
    edge: &Edge,
    landing: &Path,
    store: &Path,
    actor: &str,
    binding: Binding,
    level: Level,
    seed_notes: &[String],
) -> io::Result<()> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let user_config = edge.xdg.user_config();
    let hooks = Hooks::effective(landing, &user_config)?;
    let reg = Registry::at(landing);
    let pre = hooks.resolve(&reg, Verb::Prime.token(), "pre");
    let post = hooks.resolve(&reg, Verb::Prime.token(), "post");
    let before = EffectiveConfig::resolve(landing, &user_config)?.tasks_branch;
    let mut step = || -> io::Result<Option<String>> {
        let name = EffectiveConfig::resolve(landing, &user_config)?.tasks_branch;
        substrate::materialize(landing, store, &name, actor)?;
        Ok((name != before).then_some(name))
    };
    let log = Log::new(clone.op_log(), level, Verb::Prime, log::wall);
    // The founding seed's prune notes land here, `info` like install's dangling
    // report (bl-5b09): an actionable incompleteness report, persisted and
    // threshold-gated — not a bare eprintln the log file never sees.
    for note in seed_notes {
        log.record(Level::Info, "core", None, note);
    }
    let plugins = Subprocess::new(OpContext::diffless(actor.to_string(), binding), &log, edge.depth);
    let anvil = git::Git::at(store);
    Engine::new(&anvil, &plugins, &log)
        .prime(landing, store, &pre, &post, &mut step)
        .map_err(|e| io::Error::other(e.to_string()))
}

/// `bl sync [BRANCH] [--as ID] [--remote URL]` — make state
/// consistent (§13): run the `sync`
/// chain against the store (the tracker's `sync/pre` fetches + ff-only). With no
/// arg it syncs the config-named `tasks_branch`; `bl sync <branch>` PULLS that
/// named branch instead — the positional substitutes `tasks_branch` in the §7
/// binding, the one datum the tracker fetches/ff's against. `--remote`
/// is the per-op override tier of the ONE §12 ladder (bl-c2de), resolved here
/// exactly as on prime and the mutating verbs (`--center` is prime-only —
/// enrollment, not an override, bl-35e5). The landing is
/// never a sync target, but core special-cases no name: the landing is
/// upstream-less by construction (§4), so the tracker's general rule — fetch
/// the branch's upstream, if any — no-ops on it for free (§2/§13).
pub fn sync(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse_sync(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    if !is_landing(&landing) {
        return Err(io::Error::other("no balls checkout here — run `bl prime` first"));
    }
    let (binding, level) = bind(edge, &landing, &store, opts.remote, opts.branch)?;
    run_chain(edge, &landing, &store, Verb::Sync, &opts.actor, binding, level)
}

/// [`Level`]: the EXPLICIT store remote, the `tasks_branch` (the `target`
/// override, else the config-named one — §13 `bl sync <branch>`), the two checkout
/// paths, and the `log_level` threshold (CLI override over config). `cli_remote` is
/// the parsed `--remote` per-op override — the top tier of the ONE §12
/// ladder, accepted by every store-touching verb alike (bl-c2de). The rest of
/// core's remote handling is [`crate::config::remote_ladder`] — the landing
/// `task_remote` policy rung (the stealth sentinel, bl-9df0) over this clone's
/// `binding.toml` remote (bl-d081) over the legacy per-machine XDG `remote`, all
/// plain config reads; core never resolves an
/// implicit remote (§0). `None` here is NOT stealth: it means "no EXPLICIT remote",
/// and the binding carries `remote: None` to the tracker, which discovers the
/// project-repo `origin` (the bottom §12 tier — remote-talk, so the tracker's
/// alone). The `stealth` bit is DERIVED from the resolved sentinel — never an
/// argv fact — and rides the binding so the tracker skips that discovery;
/// `bind` is the one resolution point every op shares, which is what makes the
/// opt-out bind the checkout rather than one invocation.
pub(crate) fn bind(edge: &Edge, landing: &Path, store: &Path, cli_remote: Option<String>, target: Option<String>) -> io::Result<(Binding, Level)> {
    let user_config = edge.xdg.user_config();
    let binding_path = edge.xdg.clone_dir(&edge.invocation_path).binding();
    let cfg = EffectiveConfig::resolve(landing, &user_config)?;
    let level = Level::parse(edge.log_level.as_deref().unwrap_or(&cfg.log_level))?;
    let (remote, stealth) = crate::config::remote_ladder(cli_remote, landing, &binding_path, &user_config)?;
    let tasks_branch = target.unwrap_or(cfg.tasks_branch);
    Ok((binding(landing, store, &edge.invocation_path, remote, stealth, tasks_branch), level))
}

/// Run the DIFFLESS chain for `op` (§13): resolve the plugin sets from the
/// LANDING's `config/plugins.toml` `[hooks]` schedule (§6), then run them with
/// cwd = the STORE checkout and the anvil bracketing the store-branch HEAD.
fn run_chain(edge: &Edge, landing: &Path, store: &Path, op: Verb, actor: &str, binding: Binding, level: Level) -> io::Result<()> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let hooks = Hooks::effective(landing, &edge.xdg.user_config())?;
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
pub(crate) fn binding(landing: &Path, store: &Path, invocation: &Path, remote: Option<String>, stealth: bool, tasks_branch: String) -> Binding {
    Binding {
        remote,
        stealth, // derived from the landing sentinel by the §12 ladder, never argv (bl-9df0)
        tasks_branch,
        store: store.to_string_lossy().into_owned(),
        landing: landing.to_string_lossy().into_owned(),
        invocation_path: invocation.to_string_lossy().into_owned(),
    }
}

// The argv parsers live in a sibling module (the §9 mutate_args convention);
// `value` is re-exported because `bl install`'s parse shares it.
#[path = "checkout_args.rs"]
mod args;
use args::{parse_prime, parse_sync};
pub(crate) use args::value;

#[cfg(test)]
#[path = "checkout_tests.rs"]
mod tests;
