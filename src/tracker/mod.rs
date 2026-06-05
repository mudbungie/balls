//! The `tracker` plugin — balls' one remote-talker (§0/§12/§13).
//!
//! Base balls is local-only: it commits task-file changes to the balls branch
//! and never touches a remote. Everything beyond that is a plugin, and the
//! tracker is the plugin that owns the remote. It is a SEPARATE binary, invoked
//! subprocess-uniform (`<bin> <op> <phase>`, §6) with the §7 wire on stdin and
//! no return channel — in-repo only as a default capability + reference impl.
//!
//! Its whole job is three git acts on the state branch:
//! - [`remote_ops::sync`] — `sync/pre`: fetch + fast-forward-only import; a
//!   non-ff is the contention signal (§13).
//! - [`remote_ops::push`] — `*/post`: publish the sealed balls branch (§12).
//! - [`prime::prime`] — `prime/pre`: resolve the upstream, adopt-or-found,
//!   or write the stealth self-lock when there is no remote (§12).
//!
//! The [`pointer`] is the committed `next:` trail hop the tracker owns; the
//! wire's [`Binding`] is everything else it needs. Each handler no-ops in a
//! stealth (no-remote) repo — the structural opt-out (§12).

mod git;
mod payload;
mod pointer;
mod prime;
mod remote_ops;

#[cfg(test)]
mod fixtures;

pub use payload::Binding;

use serde::Serialize;
use std::io::{self, Read, Write};

/// The host-resolved environment the binary edge hands the tracker: the XDG
/// roots that locate this checkout's clone bundle (§1). No env reads in the lib
/// — the edge resolves them once (the bl-bfa8 rule) and passes them in.
pub struct Env {
    pub xdg: crate::layout::Xdg,
}

/// The ops the tracker handles, for the §6 `protocol` self-description: the
/// deliverable verbs (it pushes on their `post`) plus `sync`/`prime`.
const OPS: &[&str] = &[
    "create", "claim", "unclaim", "update", "close", "drop", "sync", "prime",
];

/// The §6 self-description emitted by `tracker protocol`. balls never persists
/// it; it is read at install time to validate a binding.
#[derive(Serialize)]
struct SelfDescription {
    protocol: u32,
    ops: &'static [&'static str],
}

/// The tracker entrypoint: dispatch `args` (`protocol`, or `<op> <phase>` with
/// the §7 payload on `input`), returning the process exit code. A handler error
/// is logged to stderr and becomes exit `1` — the §6 "non-zero aborts the op".
pub fn run(args: &[String], input: &mut impl Read, out: &mut impl Write, env: &Env) -> i32 {
    match dispatch(args, input, out, env) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("tracker: {e}");
            1
        }
    }
}

fn dispatch(args: &[String], input: &mut impl Read, out: &mut impl Write, env: &Env) -> io::Result<()> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["protocol"] => protocol(out),
        [op, phase] => handle(op, phase, input, env),
        _ => Err(io::Error::other("usage: tracker protocol | tracker <op> <phase>")),
    }
}

/// Emit the §6 `{ protocol, ops }` self-description as JSON.
fn protocol(out: &mut impl Write) -> io::Result<()> {
    let desc = SelfDescription { protocol: crate::message::PROTOCOL, ops: OPS };
    serde_json::to_writer(&mut *out, &desc).map_err(io::Error::other)?;
    out.write_all(b"\n")
}

/// Route one `<op> <phase>` to its handler. The tracker acts in exactly three
/// slots — `sync/pre`, `prime/pre`, and any deliverable verb's `post` — and
/// no-ops everywhere else (reads, the other phases). `sync`/`prime` are matched
/// out before the catch-all `post` so their own `post` never triggers a push.
fn handle(op: &str, phase: &str, input: &mut impl Read, env: &Env) -> io::Result<()> {
    let binding = payload::read_binding(input)?;
    match (op, phase) {
        ("sync", "pre") => remote_ops::sync(&binding),
        ("prime", "pre") => prime::prime(&binding, env),
        ("sync" | "prime", _) => Ok(()),
        (_, "post") => remote_ops::push(&binding),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn env() -> (TempDir, Env) {
        let tmp = TempDir::new().unwrap();
        let env = super::fixtures::env(&tmp.path().join("home"), &tmp.path().join("state"));
        (tmp, env)
    }

    /// A stealth binding (no remote) — every handler no-ops without git.
    fn stealth() -> String {
        r#"{"binding":{"branch":"balls","operating":"/nope","invocation_path":"/p"}}"#.to_string()
    }

    fn invoke(op: &str, phase: &str, payload: &str, env: &Env) -> i32 {
        run(&[op.to_string(), phase.to_string()], &mut payload.as_bytes(), &mut io::sink(), env)
    }

    #[test]
    fn protocol_self_describes_version_and_ops() {
        let (_tmp, env) = env();
        let mut out = Vec::new();
        let code = run(&["protocol".to_string()], &mut io::empty(), &mut out, &env);
        assert_eq!(code, 0);
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["protocol"], 1);
        let ops = v["ops"].as_array().unwrap();
        assert!(ops.iter().any(|o| o == "sync") && ops.iter().any(|o| o == "claim"));
    }

    #[test]
    fn an_unrecognized_invocation_is_a_usage_error() {
        let (_tmp, env) = env();
        assert_eq!(run(&[], &mut io::empty(), &mut io::sink(), &env), 1);
    }

    #[test]
    fn a_malformed_payload_aborts_with_exit_one() {
        let (_tmp, env) = env();
        assert_eq!(invoke("sync", "pre", "not json", &env), 1);
    }

    #[test]
    fn stealth_handlers_all_no_op_to_exit_zero() {
        let (_tmp, env) = env();
        for (op, phase) in [
            ("sync", "pre"),
            ("sync", "post"),
            ("prime", "post"),
            ("claim", "pre"),
            ("claim", "post"),
        ] {
            assert_eq!(invoke(op, phase, &stealth(), &env), 0, "{op} {phase}");
        }
    }

    #[test]
    fn prime_pre_dispatches_to_the_prime_handler() {
        let (_tmp, env) = env();
        assert_eq!(invoke("prime", "pre", &stealth(), &env), 0);
        let lock = env.xdg.clone_dir(Path::new("/p")).root().join("stealth.lock");
        assert!(lock.is_file()); // proof the prime handler, not a no-op, ran
    }
}
