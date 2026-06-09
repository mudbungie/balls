//! balls — a git-native task tracker (greenfield rewrite, spec bl-2e26).
//!
//! This is the next major version of balls, built fresh under epic bl-72a8.
//! The previous implementation is deleted from the working tree (recoverable
//! from git history); the system-installed `bl` keeps tracking this work until
//! cutover, so `main` does not need to build a functional `bl` during the
//! rewrite — do not `make install` from this tree until the rewrite lands.
//!
//! # §0 — what balls is
//!
//! State rides TWO branches of a git repo (§2): the `balls/config` landing holds
//! `config/`, the `tasks_branch` store holds `tasks/`; persistence is git,
//! local-first. Base balls is the smallest possible thing — it commits config to
//! the landing and task-file changes to the store. Everything that touches the
//! world beyond it (a remote, the project's code) is a plugin.
//!
//! # §8 — the op lifecycle is the spine
//!
//! Every verb is the same shape: balls authors a base change, an ordered
//! plugin chain acts on it, balls SEALS it (commit + integrate, atomically),
//! and plugins react. The seal is the pre/post boundary. [`op`] names the
//! verb-agnostic phase shape; [`git`] is the anvil seal (change worktree,
//! commit + ff-integrate, un-seal); [`lifecycle`] is the [`lifecycle::Engine`]
//! that runs the shape and unwinds it in reverse on any abort (§14). [`change`]
//! implements the verb diff ([`lifecycle::BaseChange`]) for each §9 deliverable
//! verb (create/claim/unclaim/update/close/drop); the plugin chain
//! ([`lifecycle::Plugins`]) is filled by [`plugin::Subprocess`] over the §7 wire
//! ([`wire`]). [`run`] dispatches the checkout-lifecycle verbs (`prime`/`sync`,
//! §12/§13) to the engine via [`checkout`], the deliverable verbs via
//! [`mutate`], and `install` — the op that seals to the LANDING (§6/§8) — via
//! [`install::run`]; every verb is wired.
//!
//! # §12/§13 — readiness & synchronization
//!
//! [`checkout`] is `bl prime` (idempotent orchestrator: bootstrap-on-miss via
//! [`substrate`] — the `balls/config` landing + the `tasks_branch` store — then
//! the prime chain) and `bl sync` (the synchronization primitive: run the sync
//! chain against the store). Core stays local-only — it reads config from the
//! landing and reads/writes tasks on the store; the [`tracker`] plugin the chain
//! runs is the one component that talks to a remote (§0). [`edge`] is the host
//! inputs `main` resolves at the boundary.
//!
//! # §6/§7 — the plugin contract
//!
//! Plugins are subprocesses, invoked uniformly (`<bin> <op> <phase>`) with the
//! §7 payload on stdin and no return channel — they mutate the change worktree,
//! never print state back. [`plugin`] is the dispatch (env, recursion guard,
//! stderr-to-logs, `protocol` self-describe); [`wire`] is the payload shape.
//! [`install`] is the §6
//! `bl install` capability transfer: a pure path-copy of a committed path between
//! two branches whose SHAPE decides the semantics (folder = mirror with deletions
//! propagating, file/glob = additive union), never touching siblings or the
//! gitignored `bin/`, then resolving + validating a local binary against its
//! `protocol` self-describe before binding it. [`tracker`] is the
//! one shipped remote-talker (a separate binary): it reads the §7 wire and does
//! the §12/§13 git acts — sync (fetch + ff-only), push on post, and prime
//! (adopt/found/stealth-lock) — and nothing local touches a remote without it.
//!
//! # §3/§10 — task files & the blocker model
//!
//! [`task`] is the schema and its derived predicates (`status`/`ready`/
//! `closeable`); [`taskfile`] is the shared `tasks/<id>.md` IO (read/write,
//! `exists` as the §10 resolver, the front-door reciprocal `add_blocker`).
//! Enforcement is CORE (§10): [`enforce`] guards `claim` on [`task::Task::ready`]
//! and `close` on [`task::Task::closeable`] (called from [`change`] at stage),
//! so a blocker actually blocks without a plugin — its meaning is enforced where
//! the op is authored.
//!
//! # §11 — the delivery / worktree plugin
//!
//! The first shipped plugin: a SIBLING binary (`bl-delivery`) that owns the
//! `work/<id>` code worktree of the PROJECT repo end to end — materialize on
//! `claim`, deliver (direct local-squash) + tear down on `close`. [`delivery`]
//! is the kind-blind, stateless-across-ops policy (the hook→act matrix + the
//! derived [`delivery::worktree_path`]); [`delivery_repo`] is its real git seam.
//! It lives in-repo as a default capability + reference impl, dispatched
//! subprocess-uniform like any third party (§6).
//!
//! # §4 — config values, read from the landing
//!
//! [`config`] is the §4 `EffectiveConfig`: the landing's `config/balls.toml`
//! overlaid by the XDG user config, with built-in defaults beneath — no trail,
//! config lives on the landing alone (§12).
//!
//! # §1/§2 — the layout substrate
//!
//! [`encoding`], [`layout`], and [`registry`] answer *where balls' state lives
//! and how it is named*: percent-encoded (never hashed) paths under the XDG
//! dirs, and the local `config/plugins/bin/<name>` binary binding (gitignored
//! absolute symlinks) that resolves the committed `config/plugins.toml` [hooks]
//! schedule to this machine. Pure path arithmetic plus the registry's
//! filesystem ops — no git, no env reads (the binary edge supplies those), no
//! bootstrap (that is prime's job).

pub mod adopt;
pub mod change;
pub mod checkout;
pub mod civil;
pub mod config;
pub mod delivery;
pub mod delivery_repo;
pub mod edge;
pub mod encoding;
pub mod enforce;
pub mod git;
pub mod help;
pub mod hooks;
pub mod id;
pub mod install;
pub mod layout;
pub mod lifecycle;
pub mod log;
pub mod message;
pub mod mutate;
pub mod op;
pub mod plugin;
pub mod reads;
pub mod registry;
pub(crate) mod safegit;
pub mod seed;
pub mod substrate;
pub mod task;
pub mod taskfile;
pub mod tracker;
pub mod verb;
pub mod wire;

use edge::Edge;
use verb::Verb;

/// The LANDING branch — path-derived, single-owner, holds `config/` (§2). It is
/// never named by config (you read config FROM it, so it cannot name where it
/// lives — §4); the one fixed point a fresh checkout bootstraps against (§12).
pub const LANDING_BRANCH: &str = "balls/config";

/// The default STORE branch — holds `tasks/` (§2). Unlike the landing this is the
/// one indirection: `config.tasks_branch` names it and may point elsewhere (§4).
/// Default-two (a DISTINCT ref) is simplest and fewest code paths (§0/§2).
pub const DEFAULT_TASKS_BRANCH: &str = "balls/tasks";

/// The agent skill guide, embedded so `bl skill` works from a bare `cargo
/// install` (no repo checkout to read it from). `skill` is help OUTPUT, not an
/// op — it authors no diff, has no lifecycle, and is never a blocker target — so
/// it is dispatched directly in [`run`] and deliberately kept OUT of the [`Verb`]
/// enum (which doubles as a blocker's `on`, §10).
const SKILL: &str = include_str!("../SKILL.md");

/// The §8 dispatch entrypoint: resolve argv to its verb and run it. `prime`/
/// `sync` (§12/§13) wire to the engine via [`checkout`]; the deliverable verbs
/// (§9) via [`mutate`]; the read verbs (`show`/`list`/`dep-tree`, §9) via
/// [`reads`] — they author no diff and print the store view; `install` (§6)
/// seals its path-copy onto the landing or store via [`install::run`]. `skill`
/// prints the embedded agent guide and `help` (also `--help`/`-h`) the terse
/// command directory ([`help::directory`]). `edge` carries the host inputs `main`
/// resolved.
///
/// Returns the process exit code: `0` on success (including `skill`/`help`), `1`
/// on an op failure (a plugin aborted, a bad flag), `2` for an unknown or missing
/// command (usage convention — the message points at `bl help`).
///
/// `--log-level LEVEL` is the §4 layer-1 CLI override (the only global flag): it
/// is stripped here from anywhere in argv and stamped onto the [`Edge`] the op
/// reads, so the per-verb parsers never see it. A trailing `--log-level` with no
/// value is a usage error (exit 2).
pub fn run(edge: &Edge, args: &[String]) -> i32 {
    let (log_level, rest) = match strip_log_level(args) {
        Ok(split) => split,
        Err(e) => {
            eprintln!("bl: {e}");
            return 2;
        }
    };
    // `skill` (full manual) and `help` (terse command directory) are help OUTPUT,
    // not ops: kept out of `Verb`, dispatched here, print to stdout, exit 0. `help`
    // also answers the conventional `--help`/`-h`.
    match rest.first().map(String::as_str) {
        Some("skill") => {
            print!("{SKILL}");
            return 0;
        }
        Some("help" | "--help" | "-h") => {
            print!("{}", help::directory());
            return 0;
        }
        _ => {}
    }
    let edge = &Edge { log_level, ..edge.clone() };
    let Some(token) = rest.first().map(String::as_str) else {
        eprintln!("usage: bl <command> — run `bl help` for the list");
        return 2;
    };
    let Some(verb) = Verb::parse(token) else {
        eprintln!("bl: unknown command '{token}' — run `bl help` for the list");
        return 2;
    };
    let result = match verb {
        Verb::Prime => checkout::prime(edge, &rest[1..]),
        Verb::Sync => checkout::sync(edge, &rest[1..]),
        Verb::Show | Verb::List | Verb::DepTree => reads::run(edge, verb, &rest[1..]),
        Verb::Install => install::run(edge, &rest[1..]),
        // Everything left is a deliverable verb (§9); mutate's own dispatch
        // still rejects a non-mutating verb defensively.
        v => mutate::run(edge, v, &rest[1..]),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            // `e` already names the verb where it adds clarity (`claim: … blocked
            // by …`, `show: needs a ball id`); the wrapper just tags it as a bl
            // error, so the verb is named ONCE — not the doubled `bl show: show:`.
            eprintln!("bl: {e}");
            1
        }
    }
}

/// Pull the global `--log-level LEVEL` flag out of argv (from any position),
/// returning the requested level and argv with the flag removed. A `--log-level`
/// with no following value is a usage error.
fn strip_log_level(args: &[String]) -> Result<(Option<String>, Vec<String>), String> {
    let mut level = None;
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--log-level" {
            i += 1;
            level = Some(args.get(i).ok_or("--log-level needs a value")?.clone());
        } else {
            rest.push(args[i].clone());
        }
        i += 1;
    }
    Ok((level, rest))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
