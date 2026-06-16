//! §9 deliverable-verb dispatch — `create`/`claim`/`unclaim`/`update`/`close`,
//! wired to the §8 engine. The MUTATING counterpart to [`crate::checkout`]
//! (which wires the diffless `prime`/`sync`): these author a `tasks/<id>.md` diff
//! and SEAL it, so they run the full Author → Pre → Seal → Post → Teardown shape
//! against a change worktree off the STORE anvil
//! ([`crate::lifecycle::Engine::seal`]).
//!
//! Every collaborator already exists — [`crate::change`] authors each verb's diff
//! ([`BaseChange`]), [`crate::lifecycle`] runs the shape with §14 rollback,
//! [`crate::plugin`] is the §6 subprocess chain over the §7 [`crate::wire`]. This
//! is the integration seam: it parses argv into a [`BaseChange`], resolves the §7
//! binding + the `[hooks]` plugin sets, INJECTS the clock, and drives the
//! engine. The §10/§15 front-door flags (`--parent` containment-only, `--blocks
//! OP`/`--blocks ID:OP`, `--needs B[:OP]`) write their `{id,on}` edges through
//! [`Create`]'s authoring — `on` is ANY op; all flag parsing is core — plugins
//! are hook binaries and never extend the parser (§10).

use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::checkout;
use crate::edge::Edge;
use crate::git::Git;
use crate::hooks::Hooks;
use crate::id::IdScheme;
use crate::lifecycle::{BaseChange, Engine};
use crate::log::{self, Log};
use crate::plugin::Subprocess;
use crate::registry::Registry;
use crate::task::Task;
use crate::verb::Verb;
use crate::wire::{Command, OpContext};

#[path = "mutate_author.rs"]
mod author;
#[path = "mutate_build.rs"]
mod build;
#[path = "mutate_edit.rs"]
mod edit;
#[path = "mutate_guards.rs"]
mod guards;
#[path = "mutate_report.rs"]
mod report;

use author::{base_change, command};

/// Run a mutating verb (§9) end to end: parse `args`, author the verb's base
/// change against the STORE checkout, and seal it onto `tasks_branch` through the
/// §8 engine + the §6 plugin chain (resolved from the LANDING `plugins.toml`
/// `[hooks]` schedule, §2/§6). The
/// checkout must already be a landing (`bl prime` founds it, §12) — a mutating op
/// never bootstraps. `verb` is guaranteed mutating by the [`crate::run`] dispatch.
/// The one host seam read here is the [`edit::Editor`] (`--edit`'s env + tty +
/// prompt input), so [`dispatch`] below stays fully injectable for tests.
pub fn run(edge: &Edge, verb: Verb, args: &[String]) -> io::Result<()> {
    dispatch(edge, verb, args, &mut edit::Editor::live())
}

/// [`run`] with the `--edit` host seam injected. An authored change is sealed;
/// a `None` from [`base_change`] (`--edit` returned an unchanged buffer) is the
/// idempotent no-op — announced there, nothing to seal here.
fn dispatch(edge: &Edge, verb: Verb, args: &[String], editor: &mut edit::Editor) -> io::Result<()> {
    let flags = parse(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    primed(&landing)?;

    let Some((base, before)) = base_change(verb, &store, &flags, now(), editor)? else {
        return Ok(());
    };
    let ctx = Op {
        actor: flags.actor.clone(),
        remote: flags.remote.clone(),
        command: command(verb, &flags),
    };
    let sha = seal_op(edge, verb, &ctx, base.as_ref(), before)?;
    report::emit(verb, &store, &sha)
}

/// What an op carries to the seal besides its [`BaseChange`]: the stamped
/// actor, the per-op §12 remote override, and the §7 `command`.
pub(crate) struct Op {
    pub actor: String,
    pub remote: Option<String>,
    pub command: Command,
}

/// A mutating op is refused before `bl prime` founded the landing (§12) — a
/// deliverable op never bootstraps.
pub(crate) fn primed(landing: &Path) -> io::Result<()> {
    if !landing.join("config").is_dir() {
        return Err(other("no balls checkout here — run `bl prime` first"));
    }
    Ok(())
}

/// Seal an authored [`BaseChange`] onto the store through the §8 engine — the
/// wiring EVERY mutating verb shares (config + log resolve, the §12 remote
/// ladder, the §6 `[hooks]` plugin sets, the anvil). The deliverable verbs
/// reach it via [`dispatch`]; `bl import` (§16) authors its own bulk change
/// and seals through the same path, so there is exactly one road to the anvil.
/// Returns the sealed sha.
pub(crate) fn seal_op(edge: &Edge, verb: Verb, op: &Op, base: &dyn BaseChange, before: Option<Task>) -> io::Result<String> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    primed(&landing)?;
    // The ONE §12 ladder, identical on every op (bl-c2de): `checkout::bind` IS
    // the resolution point — per-op `--remote`/`--center`, the landing stealth
    // sentinel, the XDG `task-remote` (§0 stays local; the tracker discovers
    // `origin` beneath). A second ladder here is exactly how the bl-9df0
    // stealth bypass happened; there is one bind, shared with the checkout verbs.
    let (binding, level) = checkout::bind(edge, &landing, &store, op.remote.clone(), None)?;
    let log = Log::new(clone.op_log(), level, verb, log::wall);
    let ctx = OpContext {
        actor: op.actor.clone(),
        binding,
        command: Some(op.command.clone()),
        before,
    };

    let hooks = Hooks::effective(&landing, &edge.xdg.user_config())?;
    let reg = Registry::at(&landing);
    let pre = hooks.resolve(&reg, verb.token(), "pre");
    let post = hooks.resolve(&reg, verb.token(), "post");
    let change_dir = clone.change(&change_token());
    let plugins = Subprocess::new(ctx, &log, edge.depth);
    let anvil = Git::at(&store);
    Engine::new(&anvil, &plugins, &log)
        .seal(base, verb, &change_dir, &pre, &post)
        .map_err(|e| other(e.to_string()))
}

// The argv→[`Flags`] front-door parse lives in a sibling module (the §9 flag
// vocabulary in one place); re-imported so the dispatch reads naturally.
#[path = "mutate_args.rs"]
mod args;
use args::{parse, Flags};

/// The SOLE site that reads the wall clock and reduces it to §3 unix seconds.
/// Injected into each [`BaseChange`] so `change.rs` stays a pure, clock-free unit
/// (`now` is a plain argument there). A pre-epoch clock — never, in practice —
/// reads 0.
fn now() -> i64 {
    #[allow(clippy::cast_possible_wrap)]
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A unique name for the ephemeral change worktree (§8/§1 — nothing keys off it),
/// drawn from the same entropy [`IdScheme`] mints ids with, so the dispatch needs
/// no second randomness primitive.
fn change_token() -> String {
    IdScheme { prefix: String::new(), length: 32, alphabet: "0123456789abcdef".to_string() }.generate()
}

/// An ad-hoc op error.
fn other(msg: impl Into<String>) -> io::Error {
    io::Error::other(msg.into())
}

#[cfg(test)]
#[path = "mutate_tests.rs"]
mod tests;
