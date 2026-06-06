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
//! All state lives on ONE branch of a git repo; persistence is git,
//! local-first. Base balls is the smallest possible thing — it commits
//! task-file changes to that branch. Everything that touches the world beyond
//! it (a remote, the project's code) is a plugin.
//!
//! # §8 — the op lifecycle is the spine
//!
//! Every verb is the same shape: balls authors a base change, an ordered
//! plugin chain acts on it, balls SEALS it (commit + integrate, atomically),
//! and plugins react. The seal is the pre/post boundary. [`op`] names the
//! verb-agnostic phase shape; [`git`] is the terminus seal (change worktree,
//! commit + ff-integrate, un-seal); [`lifecycle`] is the [`lifecycle::Engine`]
//! that runs the shape and unwinds it in reverse on any abort (§14). [`change`]
//! implements the verb diff ([`lifecycle::BaseChange`]) for each §9 deliverable
//! verb (create/claim/unclaim/update/close/drop); the plugin chain
//! ([`lifecycle::Plugins`]) is filled by [`plugin::Subprocess`] over the §7 wire
//! ([`wire`]). [`run`] dispatches the checkout-lifecycle verbs (`prime`/`sync`,
//! §12/§13) to the engine via [`checkout`]; the deliverable verbs are not wired
//! yet, so they still print their [`op::Op`] plan.
//!
//! # §12/§13 — readiness & synchronization
//!
//! [`checkout`] is `bl prime` (idempotent orchestrator: bootstrap-on-miss via
//! [`substrate`], the trail pointer, then the prime chain) and `bl sync` (the
//! synchronization primitive: walk the [`trail`] to its terminus, run the sync
//! chain). Core stays local-only — it walks LOCAL checkouts and commits config;
//! the [`tracker`] plugin the chain runs is the one component that talks to a
//! remote (§0). [`edge`] is the host inputs `main` resolves at the boundary.
//!
//! # §6/§7 — the plugin contract
//!
//! Plugins are subprocesses, invoked uniformly (`<bin> <op> <phase>`) with the
//! §7 payload on stdin and no return channel — they mutate the change worktree,
//! never print state back. [`plugin`] is the dispatch (env, recursion guard,
//! stderr-to-logs, `protocol` self-describe); [`wire`] is the payload shape.
//! [`gate`] is the one shipped plugin (its own `gate` binary): the §10 SOLE
//! enforcer of the blocker model, which core only stores. [`install`] is the §6
//! `bl install` capability transfer: it copies the committed wiring + config
//! subtree between two `balls` branches (the plugins object mirrors only
//! relative-symlink wiring, so `bin/` and the trail pointer never travel),
//! unions `tasks/` on migration, and resolves + validates a local binary
//! against its `protocol` self-describe before binding it. [`tracker`] is the
//! one shipped remote-talker (a separate binary): it reads the §7 wire and does
//! the §12/§13 git acts — sync (fetch + ff-only), push on post, and prime
//! (adopt/found/stealth-lock) — and nothing local touches a remote without it.
//!
//! # §3/§10 — task files & the blocker model
//!
//! [`task`] is the schema and its derived predicates (`status`/`ready`/
//! `closeable`); [`taskfile`] is the shared `tasks/<id>.md` IO (read/write,
//! `exists` as the §10 resolver, the front-door reciprocal `add_blocker`).
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
//! # §1/§2 — the layout substrate
//!
//! [`encoding`], [`layout`], and [`registry`] answer *where balls' state lives
//! and how it is named*: percent-encoded (never hashed) paths under the XDG
//! dirs, and the `config/plugins/` symlink registry on the `balls` branch.
//! Pure path arithmetic plus the registry's filesystem ops — no git, no env
//! reads (the binary edge supplies those), no bootstrap (that is prime's job).
//!
//! # §16 — drift diagnosis
//!
//! [`doctor`] is base balls' half of the `doctor` read op: it audits only
//! core-owned structure (stale change worktrees, an unresolved `operating/`, a
//! `bin/` dangle, protocol drift, circular blockers) and names the existing
//! verb that fixes each — there is no `repair` verb. Plugins audit their own §1
//! territory through the `doctor` hook dirs, like any diffless op's chain.

pub mod change;
pub mod checkout;
pub mod delivery;
pub mod delivery_repo;
pub mod doctor;
pub mod edge;
pub mod encoding;
pub mod gate;
pub mod git;
pub mod id;
pub mod install;
pub mod layout;
pub mod lifecycle;
pub mod message;
pub mod op;
pub mod plugin;
pub mod registry;
pub mod substrate;
pub mod task;
pub mod taskfile;
pub mod tracker;
pub mod trail;
pub mod verb;
pub mod wire;

use edge::Edge;
use op::Op;
use verb::Verb;

/// The state branch every checkout roots its task store + config on (§2). One
/// name, compiled in — the bootstrap fact a fresh checkout needs (§12).
pub const STATE_BRANCH: &str = "balls";

/// The §8 dispatch entrypoint: resolve argv to its verb and run it. The
/// checkout-lifecycle verbs (`prime`/`sync`, §12/§13) are wired to the engine
/// via [`checkout`]; the remaining verbs are not yet, so they report their §8 op
/// plan (the skeleton dispatch). `edge` carries the host inputs `main` resolved.
///
/// Returns the process exit code: `0` on success, `1` on an op failure (a plugin
/// aborted, a bad flag), `2` for an unknown or missing verb (usage convention).
pub fn run(edge: &Edge, args: &[String]) -> i32 {
    let Some(verb) = args.first().map(String::as_str).and_then(Verb::parse) else {
        eprintln!("usage: bl <verb>");
        return 2;
    };
    let result = match verb {
        Verb::Prime => checkout::prime(edge, &args[1..]),
        Verb::Sync => checkout::sync(edge, &args[1..]),
        other => {
            println!("{}", Op::new(other).plan());
            Ok(())
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("bl {}: {e}", verb.token());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    /// An edge rooted in `tmp` with no tracker installed (stealth) — prime founds
    /// substrate and runs an empty chain, so `run` needs no plugin subprocess.
    fn edge(tmp: &TempDir) -> Edge {
        Edge {
            xdg: layout::Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
            invocation_path: tmp.path().join("proj"),
            default_actor: "tester".into(),
            depth: 0,
            tracker_bin: None,
        }
    }

    fn run_in(tmp: &TempDir, args: &[&str]) -> i32 {
        run(&edge(tmp), &args.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn a_skeleton_verb_reports_its_op_plan() {
        assert_eq!(run_in(&TempDir::new().unwrap(), &["claim"]), 0);
    }

    #[test]
    fn run_rejects_an_unknown_verb() {
        assert_eq!(run_in(&TempDir::new().unwrap(), &["frobnicate"]), 2);
    }

    #[test]
    fn run_rejects_missing_verb() {
        assert_eq!(run_in(&TempDir::new().unwrap(), &[]), 2);
    }

    #[test]
    fn prime_founds_a_landing_then_converges_on_re_run() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
        let operating = edge(&tmp).xdg.clone_dir(Path::new(&edge(&tmp).invocation_path)).operating();
        assert!(operating.join("config").join("balls.toml").is_file());
        // Idempotent: a second prime is a no-op-converge, not an error (§12).
        assert_eq!(run_in(&tmp, &["prime"]), 0);
    }

    #[test]
    fn sync_before_prime_is_an_error() {
        assert_eq!(run_in(&TempDir::new().unwrap(), &["sync"]), 1);
    }

    #[test]
    fn sync_after_prime_walks_the_trail_to_its_terminus() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(run_in(&tmp, &["prime"]), 0);
        // Stealth landing == terminus; the empty sync chain converges.
        assert_eq!(run_in(&tmp, &["sync"]), 0);
        assert_eq!(run_in(&tmp, &["sync", "landing"]), 0); // landing is never a target
    }

    #[test]
    fn a_bad_flag_is_an_op_error() {
        assert_eq!(run_in(&TempDir::new().unwrap(), &["prime", "--center"]), 1);
    }
}
