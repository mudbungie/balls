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
//! verb (create/claim/unclaim/update/close); the plugin chain
//! ([`lifecycle::Plugins`]) is filled by [`plugin::Subprocess`] over the §7 wire
//! ([`wire`]). [`run`] dispatches the checkout-lifecycle verbs (`prime`/`sync`,
//! §12/§13) to the engine via [`checkout`], the deliverable verbs via
//! [`mutate`], `import` — the §16 write inverse of the bedrock read — via
//! [`import::run`], and `install` — the op that seals to the LANDING (§6/§8) —
//! via [`install::run`]; every verb is wired.
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
//! (adopt/found, stealth no-op) — and nothing local touches a remote without it.
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
//! derived [`delivery_path::worktree_path`]); [`delivery_repo`] is its real git seam.
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
pub mod chore;
pub mod chore_cli;
pub mod civil;
pub mod conf;
pub mod config;
pub mod delivery;
pub mod delivery_fold;
pub mod delivery_message;
pub mod delivery_path;
pub mod delivery_precondition;
pub mod delivery_prune;
pub mod delivery_reconcile;
pub mod delivery_repo;
pub mod delivery_standing;
pub mod delivery_wire;
pub mod dispatch;
pub mod edge;
pub mod encoding;
pub mod enforce;
pub mod git;
pub mod help;
pub mod hooks;
pub mod id;
pub mod import;
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
pub mod renames;
pub(crate) mod safegit;
pub mod seed;
pub mod skill;
pub mod substrate;
pub mod task;
pub mod taskfile;
pub mod tracker;
pub mod verb;
pub mod wire;

/// The §8 dispatch entrypoint, re-exported as `balls::run` — the one symbol the
/// `bl` binary calls (its logic lives in [`dispatch`]).
pub use dispatch::run;

/// The LANDING branch — path-derived, single-owner, holds `config/` (§2). It is
/// never named by config (you read config FROM it, so it cannot name where it
/// lives — §4); the one fixed point a fresh checkout bootstraps against (§12).
pub const LANDING_BRANCH: &str = "balls/config";

/// The default STORE branch — holds `tasks/` (§2). Unlike the landing this is the
/// one indirection: `config.tasks_branch` names it and may point elsewhere (§4).
/// Default-two (a DISTINCT ref) is simplest and fewest code paths (§0/§2).
pub const DEFAULT_TASKS_BRANCH: &str = "balls/tasks";

/// A USAGE error — the user's argv is malformed. Tagged
/// [`std::io::ErrorKind::InvalidInput`] so [`run`] knows to print the command's
/// help after it (and only after these — an operational failure stays terse).
/// The one taxonomy bit the per-verb parsers raise; everything else is `Other`.
pub(crate) fn usage(msg: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg.into())
}
