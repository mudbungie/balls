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
//! [`crate::change::Create`]'s authoring — `on` is ANY op; all flag parsing is core — plugins
//! are hook binaries and never extend the parser (§10).

use std::io;
use std::path::Path;

use crate::checkout;
use crate::clock;
use crate::delivery_repo::Project;
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
#[path = "mutate_unsealed.rs"]
pub(crate) mod unsealed;

use author::{base_change, command, Authored};

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

    // This checkout's remote-free repo identities (bl-0161): the SET of roots
    // reachable from HEAD. `create` stamps the first (canonical) one; `claim`
    // admits a ball recorded against ANY of them; the other verbs ignore it, so
    // skip the full-history root walk for them (bl-9bee). Empty off a checkout
    // with no code repo.
    let roots = match verb {
        Verb::Create | Verb::Claim => Project::at(&edge.invocation_path).root_commits(),
        _ => Vec::new(),
    };
    // The op reads the clock ONCE (§8, bl-8b98): this instant stamps the
    // frontmatter here AND — threaded into `seal_op` — the store seal commit and
    // every plugin's spawn env (so the delivery squash agrees), three-to-one.
    let instant = clock::for_op(edge)?;
    let Some(Authored { base, before, id }) = base_change(verb, &store, &flags, instant.t, roots, editor)? else {
        return Ok(());
    };
    // The stale-read CAS (bl-9f1d): close refuses iff the task file changed
    // since this actor's own last touch AND no seen-token matches — the refusal
    // prints the unseen diff and mints the retry's token itself. Ordered before
    // the engine so a refusal costs no worktree and no plugin chain; any store
    // movement AFTER the check still aborts at the seal's ff-only integrate.
    let consumed = match verb {
        Verb::Close => crate::seen::guard(&store, &edge.invocation_path, &id, &flags.actor)?,
        _ => Vec::new(),
    };
    // The §11 delivery target (bl-7b71), derived from the graph at op time and
    // never stored: a ball that close-gates its live parent delivers into that
    // parent's ref, so `claim` forks it and `close` folds back into it. `None`
    // — every flat ball, and `create` (no ball yet) — is the integration branch.
    let target = crate::target::derive(&store, &id, before.as_ref());
    let ctx = Op {
        actor: flags.actor.clone(),
        remote: flags.remote.clone(),
        command: command(verb, &flags, target.clone(), id.clone()),
    };
    // A close's two acts are not atomic against a concurrent `bl`: the delivery
    // squash lands in `close.pre`, the seal onto the store follows. An abort
    // between them says "nothing was written" — of the STORE. `unsealed::amend`
    // asks the project repo whether the code in fact landed and, only then,
    // says so (bl-739b). Not a retry: §14 converge-on-retry stands.
    let sha = seal_op(edge, verb, &ctx, base.as_ref(), before, &instant)
        .map_err(|e| unsealed::amend(e, &edge.invocation_path, verb, &id, target.as_deref()))?;
    crate::seen::consume(&consumed); // spent only on a successful seal
    // The op's OWN id goes to the report — it named this ball before it sealed,
    // so there is nothing to re-derive (`create`, whose id a `create/pre` plugin
    // may still have reassigned, is the one exception and re-reads there).
    report::emit(verb, &store, &id, &sha)
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
pub(crate) fn seal_op(edge: &Edge, verb: Verb, op: &Op, base: &dyn BaseChange, before: Option<Task>, instant: &clock::Instant) -> io::Result<String> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    primed(&landing)?;
    // The ONE §12 ladder, identical on every op (bl-c2de): `checkout::bind` IS
    // the resolution point — per-op `--remote`, the landing stealth
    // sentinel, the XDG `task-remote` (§0 stays local; the tracker discovers
    // `origin` beneath). A second ladder here is exactly how the bl-9df0
    // stealth bypass happened; there is one bind, shared with the checkout verbs.
    let (binding, level) = checkout::bind(edge, &landing, &store, op.remote.clone(), None)?;
    let log = Log::new(clone.op_log(), level, verb, log::wall);
    // A fail-open clock note (a configured provider that could not be honoured,
    // §8) lands in the op log like any record — threshold-gated and persisted,
    // not a bare stderr line (bl-bfcc).
    if let Some(note) = &instant.note {
        log.record(log::Level::Info, "core", None, note);
    }
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
    // The op instant dates the store seal (core's own commit) and rides into every
    // plugin's spawn env so the delivery squash inherits it (§8) — three-to-one.
    let plugins = Subprocess::new(ctx, &log, edge.depth).dated(instant.t);
    let anvil = Git::at(&store).dated(instant.t);
    Engine::new(&anvil, &plugins, &log)
        .seal(base, verb, &change_dir, &pre, &post)
        .map_err(|e| other(e.to_string()))
}

// The argv→[`Flags`] front-door parse lives in a sibling module (the §9 flag
// vocabulary in one place); re-imported so the dispatch reads naturally.
#[path = "mutate_args.rs"]
mod args;
use args::{parse, Flags};

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
