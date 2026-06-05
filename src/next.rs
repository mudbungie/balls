//! Greenfield balls: the next major version of the same `balls` crate / `bl`
//! binary (spec bl-2e26, epic bl-72a8).
//!
//! This subtree is built ALONGSIDE the current implementation: the legacy
//! modules keep `bl` self-hosting while each greenfield phase lands here. At
//! cutover (bl-0802) the legacy modules are deleted, this subtree promotes to
//! the crate root, and the crate's major version bumps. Until then `next::run`
//! is reachable as public API and exercised by tests, not yet wired to the
//! `bl` binary's `main`.
//!
//! # §0 — what this is the skeleton of
//!
//! balls is a git-native task tracker. All state lives on ONE branch of a git
//! repo; persistence is git, local-first. Base balls is the smallest possible
//! thing — everything that touches the world beyond that branch is a plugin.
//!
//! # §8 — the op lifecycle is the spine
//!
//! Every verb is the same shape: balls authors a base change, an ordered
//! plugin chain acts on it, balls SEALS it (commit + integrate, atomically),
//! and plugins react. The seal is the pre/post boundary. [`run`] is that
//! dispatch in skeleton form — it resolves a verb to its [`op::Op`] and the
//! lifecycle that op will run. No phase does any work yet: the phases are the
//! seam later phases implement.

pub mod op;
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
