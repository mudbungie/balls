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
//! ([`lifecycle::Plugins`]) is a seam a later phase fills. [`run`] is still the
//! skeleton dispatch — it prints the [`op::Op`] plan; wiring it to the engine is
//! a later phase.
//!
//! # §1/§2 — the layout substrate
//!
//! [`encoding`], [`layout`], and [`registry`] answer *where balls' state lives
//! and how it is named*: percent-encoded (never hashed) paths under the XDG
//! dirs, and the `config/plugins/` symlink registry on the `balls` branch.
//! Pure path arithmetic plus the registry's filesystem ops — no git, no env
//! reads (the binary edge supplies those), no bootstrap (that is prime's job).

pub mod change;
pub mod encoding;
pub mod git;
pub mod id;
pub mod layout;
pub mod lifecycle;
pub mod message;
pub mod op;
pub mod registry;
pub mod task;
pub mod verb;

use op::Op;
use verb::Verb;

/// The §8 dispatch entrypoint, skeleton form: resolve argv to the op it names
/// and report the lifecycle that op will run.
///
/// Returns the process exit code — `0` for a recognized verb, `2` for an
/// unknown or missing one (the usage-error convention).
pub fn run(args: &[String]) -> i32 {
    if let Some(verb) = args.first().map(String::as_str).and_then(Verb::parse) {
        println!("{}", Op::new(verb).plan());
        0
    } else {
        eprintln!("usage: bl <verb>");
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_resolves_a_known_verb() {
        assert_eq!(run(&["claim".to_string()]), 0);
    }

    #[test]
    fn run_rejects_an_unknown_verb() {
        assert_eq!(run(&["frobnicate".to_string()]), 2);
    }

    #[test]
    fn run_rejects_missing_verb() {
        assert_eq!(run(&[]), 2);
    }
}
