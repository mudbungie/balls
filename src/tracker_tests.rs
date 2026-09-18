//! Tests for the tracker entrypoint — the §6 protocol self-description, the
//! `<op> <phase>` dispatch table (stealth no-ops included), and the §12
//! effective-remote resolution (explicit binding > discovered project origin).

use super::*;
use tempfile::TempDir;
use std::path::Path;

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
fn the_version_arm_names_this_binary_and_its_crate_version() {
    // A sibling states its OWN version and no other's: the set beside `bl` is
    // checked by asking each binary, not by any one of them speaking for the
    // rest (`scripts/deploy/bl-update` composes the readings).
    let (_tmp, env) = env();
    for spelling in ["--version", "-V"] {
        let mut out = Vec::new();
        assert_eq!(run(&[spelling.to_string()], &mut io::empty(), &mut out, &env), 0);
        assert_eq!(String::from_utf8(out).unwrap().trim(), crate::version::plugin_line("bl-tracker"));
    }
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
    // A tracked prime/pre CLONES IN the established remote store branch —
    // proof the (prime, pre) arm reaches prime::prime, not the catch-all no-op
    // (the old proof, the stealth self-lock, was deleted by bl-9df0).
    let (tmp, env) = env();
    let remote = super::fixtures::remote_with_branch(tmp.path());
    let landing = super::fixtures::landing_repo(tmp.path());
    let payload = format!(
        r#"{{"binding":{{"remote":"{}","tasks_branch":"{}","store":"/nope","landing":"{}","invocation_path":"/p"}}}}"#,
        remote.display(),
        super::fixtures::BRANCH,
        landing.display(),
    );
    assert_eq!(invoke("prime", "pre", &payload, &env), 0);
    assert_eq!(
        super::fixtures::tip(&landing, super::fixtures::BRANCH),
        super::fixtures::tip(&remote, super::fixtures::BRANCH)
    );
}

#[test]
fn prime_post_dispatches_to_the_content_handler() {
    // A tracked prime/post FOUNDS the absent remote branch (the founding push) —
    // proof the (prime, post) arm reaches prime_post, not the sync/prime/install
    // catch-all no-op right below it.
    let (tmp, env) = env();
    let remote = super::fixtures::empty_remote(tmp.path());
    let store = super::fixtures::local_unpushed(tmp.path());
    let payload = format!(
        r#"{{"binding":{{"remote":"{}","tasks_branch":"{}","store":"{}","invocation_path":"/p"}}}}"#,
        remote.display(),
        super::fixtures::BRANCH,
        store.display(),
    );
    assert_eq!(invoke("prime", "post", &payload, &env), 0);
    assert_eq!(
        super::fixtures::tip(&remote, super::fixtures::BRANCH),
        super::fixtures::tip(&store, "HEAD")
    );
}

#[test]
fn sync_and_install_post_never_push_a_tracked_store() {
    // The (sync|prime|install, _) catch-all keeps the non-deliverable ops off
    // the generic `post` push: a tracked store sitting AHEAD of its remote stays
    // unpushed through sync/post and install/post — in particular `install`
    // adopts config INTO the landing and must NEVER push it back out (§6/§13).
    let (tmp, env) = env();
    let remote = super::fixtures::remote_with_branch(tmp.path());
    let store = super::fixtures::store_clone(tmp.path(), &remote);
    let before = super::fixtures::tip(&remote, super::fixtures::BRANCH);
    super::fixtures::commit(&store, "local.txt", "local"); // the undelivered tip
    let payload = format!(
        r#"{{"binding":{{"remote":"{}","tasks_branch":"{}","store":"{}","invocation_path":"/p"}}}}"#,
        remote.display(),
        super::fixtures::BRANCH,
        store.display(),
    );
    for op in ["sync", "install"] {
        assert_eq!(invoke(op, "post", &payload, &env), 0, "{op} post");
        assert_eq!(super::fixtures::tip(&remote, super::fixtures::BRANCH), before, "{op} post pushed");
    }
}

#[test]
fn effective_remote_prefers_explicit_then_discovers_the_project_origin() {
    // §12 precedence FROM THE TRACKER's seat: explicit binding `remote` (core
    // resolved --remote/XDG) > the discovered project `origin`.
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    super::git::git(&proj, &["init", "-q"]).unwrap();
    super::git::git(&proj, &["remote", "add", "origin", "git@hub:proj"]).unwrap();
    let discover = Binding {
        remote: None,
        stealth: false,
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
    // A DECLARED stealth binding (`bl prime --stealth`, §12) resolves no remote
    // at all — the opt-out beats even a discoverable origin.
    let opted_out = Binding { stealth: true, ..discover.clone() };
    assert_eq!(effective_remote(&opted_out), None);
    // A path with no origin (or no repo) → None = stealth.
    let stealth = Binding { invocation_path: "/p".into(), ..discover };
    assert_eq!(effective_remote(&stealth), None);
}

#[test]
fn a_declared_stealth_binding_founds_nothing_on_a_discoverable_origin() {
    // §12/bl-9df0: a binding carrying `stealth` (core derived the landing
    // sentinel) forces the same path the inferred no-remote case takes, even
    // though the project's `origin` IS discoverable — `prime/pre` warns and
    // persists nothing, `prime/post` founds and pushes NOTHING. This is the
    // gate EVERY op's `*/post` push shares, so a post-stealth mutate can no
    // longer rediscover origin and found the store (the bl-9df0 bug).
    let (tmp, env) = env();
    let remote = super::fixtures::empty_remote(tmp.path());
    let store = super::fixtures::local_unpushed(tmp.path());
    let proj = tmp.path().join("proj");
    std::fs::create_dir(&proj).unwrap();
    super::git::git(&proj, &["init", "-q"]).unwrap();
    super::git::git(&proj, &["remote", "add", "origin", &remote.to_string_lossy()]).unwrap();
    let payload = format!(
        r#"{{"binding":{{"stealth":true,"tasks_branch":"{}","store":"{}","invocation_path":"{}"}}}}"#,
        super::fixtures::BRANCH,
        store.display(),
        proj.display(),
    );
    assert_eq!(invoke("prime", "pre", &payload, &env), 0);
    assert_eq!(invoke("prime", "post", &payload, &env), 0);
    assert_eq!(invoke("create", "post", &payload, &env), 0); // a mutate op's publish
    // The origin still carries no store branch: nothing was founded or pushed.
    assert!(super::git::git(&remote, &["rev-parse", super::fixtures::BRANCH]).is_err());
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

/// bl-1266/bl-aac7 — the held-chain parse lives in the lib (the bl-bfa8 rule)
/// and FAILS OPEN: a tracker run by hand, or by a core too old to set the
/// variable, must publish rather than silently stop federating.
#[test]
fn env_resolve_parses_the_held_chain_and_fails_open() {
    let xdg = || crate::layout::Xdg::with(Path::new("/h"), None, None);
    assert!(Env::resolve(xdg(), None).held.is_empty()); // unset ⇒ publishes
    let held = Env::resolve(xdg(), Some("/a/tasks:/b/tasks".into())).held;
    assert_eq!(held, [Path::new("/a/tasks"), Path::new("/b/tasks")]);
}

/// The §12 rung itself, store-scoped: the chain ends with the spawner's own
/// store, so "nested" reads as that store appearing ABOVE the spawner — and a
/// different store above it is somebody else's anvil, not this op's parent.
#[test]
fn an_op_publishes_unless_an_enclosing_bl_holds_its_store() {
    let nested = |chain: &[&str], store| super::fixtures::env_held(&chain.iter().map(Path::new).collect::<Vec<_>>()).nested(store);
    assert!(!nested(&[], "/s"), "a hand-run tracker fails open");
    assert!(!nested(&["/s"], "/s"), "spawned by a top-level bl — this op's push is its own");
    assert!(nested(&["/s", "/s"], "/s"), "spawned by a bl a plugin shelled on the same store — the parent publishes");
    assert!(!nested(&["/s", "/t"], "/t"), "a nested bl -C on an unheld store publishes (H1)");
    assert!(nested(&["/s", "/t", "/s"], "/s"), "held two levels up is still held");
}
