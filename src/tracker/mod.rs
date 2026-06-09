//! The `tracker` plugin — balls' one remote-talker (§0/§12/§13).
//!
//! Base balls is local-only: it commits task-file changes to the balls branch
//! and never touches a remote. Everything beyond that is a plugin, and the
//! tracker is the plugin that owns the remote. It is a SEPARATE binary, invoked
//! subprocess-uniform (`<bin> <op> <phase>`, §6) with the §7 wire on stdin and
//! no return channel — in-repo only as a default capability + reference impl.
//!
//! Its whole job is git acts on the state branch:
//! - [`remote_ops::sync`] — `sync/pre`: fetch + fast-forward-only import; a
//!   non-ff is the contention signal (§13).
//! - [`remote_ops::push`] — `*/post`: publish the sealed balls branch (§12).
//! - [`prime::prime`] — `prime/pre`: settle the store name, clone an established
//!   remote branch into a local ref, or write the stealth self-lock (§12).
//! - [`prime::prime_post`] — `prime/post`: settle content — fetch-ff an
//!   established remote then push, or found an absent branch by pushing (§12).
//!
//! The wire's [`Binding`] is everything it needs — `remote` + `tasks_branch`
//! name the store upstream DIRECTLY, with no trail to walk (§12). When the binding
//! carries no explicit `remote` (core resolves only `--remote`/`--center`/XDG, the
//! config tiers — §0 keeps it local-only), the tracker discovers the project-repo
//! `origin` as its single fallback ([`effective_remote`], resolved once at the
//! [`handle`] dispatch point). Each handler no-ops in a stealth repo — no explicit
//! remote AND no discoverable origin — the structural opt-out (§12).

mod git;
mod payload;
mod prime;
mod remote_ops;

#[cfg(test)]
mod fixtures;

pub use payload::Binding;

use serde::Serialize;
use std::io::{self, Read, Write};
use std::path::Path;

/// The host-resolved environment the binary edge hands the tracker: the XDG
/// roots that locate this checkout's clone bundle (§1). No env reads in the lib
/// — the edge resolves them once (the bl-bfa8 rule) and passes them in.
pub struct Env {
    pub xdg: crate::layout::Xdg,
}

/// The ops the tracker handles, for the §6 `protocol` self-description: the
/// deliverable verbs (it pushes on their `post`), `sync`/`prime`, and `install`
/// (it fetches the center's config on `install/pre`, §13).
const OPS: &[&str] = &[
    "create", "claim", "unclaim", "update", "close", "drop", "sync", "prime", "install",
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

/// Route one `<op> <phase>` to its handler. The tracker acts in five slots —
/// `sync/pre`, `prime/pre` (settle name + clone-in), `prime/post` (settle content
/// — fetch-ff + push, bl-0a23), `install/pre` (the §13 config fetch), and any
/// deliverable verb's `post` (the push) — and no-ops everywhere else (reads, the
/// other phases). `prime/post` is matched out explicitly BEFORE the
/// `sync`/`prime`/`install` catch-all so it reaches its own content handler; that
/// catch-all then keeps `sync`/`install` from triggering the generic `post` push:
/// in particular `install` adopts config INTO the local landing (a fetch), and
/// must NEVER push the landing back out (publishing is a separate direction,
/// §6/§13).
fn handle(op: &str, phase: &str, input: &mut impl Read, env: &Env) -> io::Result<()> {
    let mut binding = payload::read_binding(input)?;
    binding.remote = effective_remote(&binding);
    match (op, phase) {
        ("sync", "pre") => remote_ops::sync(&binding),
        ("prime", "pre") => prime::prime(&binding, env),
        ("prime", "post") => prime::prime_post(&binding, env),
        ("install", "pre") => remote_ops::fetch_config(&binding),
        ("sync" | "prime" | "install", _) => Ok(()),
        (_, "post") => remote_ops::push(&binding),
        _ => Ok(()),
    }
}

/// The effective store remote for this op (§12): the EXPLICIT remote core already
/// resolved (`--remote`/`--center`/XDG `remote`, on the binding), else the
/// auto-discovered project-repo `origin`. Implicit `origin` discovery is the
/// TRACKER's alone — core stays local-only (§0) and hands a `remote: None`
/// binding when no explicit tier is set. Resolved ONCE here at the [`handle`]
/// dispatch point and written back onto `binding.remote`, so every handler shares
/// this one fallback and reads `binding.remote` as before — the stealth gate
/// ("no remote ⇒ no-op") thus means "no explicit remote AND no discoverable
/// origin", with no per-handler re-probe.
fn effective_remote(b: &Binding) -> Option<String> {
    b.remote.clone().or_else(|| origin_of(Path::new(&b.invocation_path)))
}

/// The auto-discovered store remote — `git remote get-url origin` on the PROJECT
/// repo (the `invocation_path`, the clone the user works in, whose `origin` is the
/// real upstream the code rides and where `balls/tasks` sits alongside it). A
/// LOCAL config read, no network. NEVER the landing: the landing is local-only
/// (§2 install-transport, founded by a bare `git init`) and carries no origin.
/// Absent origin (the stealth case, or a non-repo path) ⇒ `None`.
fn origin_of(project: &Path) -> Option<String> {
    git::git(project, &["remote", "get-url", "origin"]).ok()
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
        r#"{"binding":{"tasks_branch":"balls/tasks","store":"/nope","invocation_path":"/p"}}"#.to_string()
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
            ("install", "pre"),
            ("install", "post"), // must NOT push the landing back out
        ] {
            assert_eq!(invoke(op, phase, &stealth(), &env), 0, "{op} {phase}");
        }
    }

    #[test]
    fn install_pre_dispatches_to_the_config_fetch() {
        let (tmp, env) = env();
        let center = super::fixtures::remote_with_config(tmp.path(), "balls/shared");
        let landing = super::fixtures::local_unpushed(tmp.path());
        let payload = format!(
            r#"{{"binding":{{"remote":"{}","tasks_branch":"balls/tasks","store":"/nope","landing":"{}","invocation_path":"/p"}}}}"#,
            center.display(),
            landing.display(),
        );
        assert_eq!(invoke("install", "pre", &payload, &env), 0);
        // Proof the fetch handler ran (not the catch-all): FETCH_HEAD now resolves.
        assert!(super::git::git(&landing, &["rev-parse", "FETCH_HEAD"]).is_ok());
    }

    #[test]
    fn prime_pre_dispatches_to_the_prime_handler() {
        let (_tmp, env) = env();
        assert_eq!(invoke("prime", "pre", &stealth(), &env), 0);
        let lock = env.xdg.clone_dir(Path::new("/p")).root().join("stealth.lock");
        assert!(lock.is_file()); // proof the prime handler, not a no-op, ran
    }

    #[test]
    fn effective_remote_prefers_explicit_then_discovers_the_project_origin() {
        // §12 precedence FROM THE TRACKER's seat: explicit binding `remote` (core
        // resolved --remote/--center/XDG) > the discovered project `origin`.
        let tmp = TempDir::new().unwrap();
        let proj = tmp.path().join("proj");
        std::fs::create_dir(&proj).unwrap();
        super::git::git(&proj, &["init", "-q"]).unwrap();
        super::git::git(&proj, &["remote", "add", "origin", "git@hub:proj"]).unwrap();
        let discover = Binding {
            remote: None,
            tasks_branch: "balls/tasks".into(),
            store: "/nope".into(),
            landing: String::new(),
            invocation_path: proj.to_string_lossy().into_owned(),
        };
        // No explicit remote → discover the PROJECT repo's origin (invocation_path).
        assert_eq!(effective_remote(&discover).as_deref(), Some("git@hub:proj"));
        // An explicit remote wins, never probed.
        let explicit = Binding { remote: Some("git@hub:explicit".into()), ..discover.clone() };
        assert_eq!(effective_remote(&explicit).as_deref(), Some("git@hub:explicit"));
        // A path with no origin (or no repo) → None = stealth.
        let stealth = Binding { invocation_path: "/p".into(), ..discover };
        assert_eq!(effective_remote(&stealth), None);
    }

    #[test]
    fn dispatch_discovers_the_project_origin_when_no_explicit_remote() {
        // The wire-up: a binding with `remote: None` but a project repo whose
        // `origin` is the store's upstream → handle() discovers it ONCE at the
        // dispatch point (the one place all handlers share), so `sync/pre` takes
        // the TRACKED path and fast-forwards. Proof discovery is wired at dispatch
        // and reads the invocation_path — not a stealth no-op.
        let (tmp, env) = env();
        let remote = super::fixtures::remote_with_branch(tmp.path());
        let store = super::fixtures::store_clone(tmp.path(), &remote);
        // Advance the remote out from under the store.
        let other = super::fixtures::checkout(tmp.path(), &remote, "other");
        let moved = super::fixtures::commit(&other, "next.txt", "next");
        super::git::git(&other, &["push", "-q", "origin", super::fixtures::BRANCH]).unwrap();
        // The project repo bl was invoked from, whose `origin` is that remote.
        let proj = super::fixtures::checkout(tmp.path(), &remote, "proj");
        let payload = format!(
            r#"{{"binding":{{"tasks_branch":"{}","store":"{}","invocation_path":"{}"}}}}"#,
            super::fixtures::BRANCH,
            store.display(),
            proj.display(),
        );
        assert_eq!(invoke("sync", "pre", &payload, &env), 0);
        assert_eq!(super::fixtures::tip(&store, "HEAD"), moved); // discovered origin → ff'd
    }
}
