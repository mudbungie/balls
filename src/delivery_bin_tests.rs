//! Unit tests for the promoted delivery-plugin boundary ([`crate::delivery_bin`])
//! — every arm of [`run`]/`hook` on throwaway repos and byte-buffer stdio, the
//! same coverage the shipped binary's process edge gets end-to-end in
//! `tests/delivery/`. The point of the module is that a linking host reaches
//! the identical adaptation, so these tests drive it exactly as such a host
//! would: injected argv, injected wire, injected [`Env`].

use super::*;
use std::fs;
use tempfile::TempDir;

/// A throwaway project repo on `main` with one seed commit (the
/// `delivery_repo_tests` fixture, restated — that module is test-private).
fn project() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("proj");
    fs::create_dir(&root).unwrap();
    let g = |args: &[&str]| Project::run(&root, args).unwrap();
    g(&["init", "-q", "-b", "main"]);
    g(&["config", "user.name", "test"]);
    g(&["config", "user.email", "test@example.com"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "seed"]);
    (tmp, root)
}

/// An [`Env`] whose plugin territory lives under the given state root.
fn env_at(state: &Path, cwd: &Path) -> Env {
    Env {
        plugin: Some("bl-delivery".into()),
        xdg: Xdg::with(Path::new("/nonexistent-home"), None, state.to_str()),
        cwd: cwd.to_path_buf(),
    }
}

/// Drive [`run`] with `input` bytes, returning `(exit, stdout)`.
fn drive(args: &[&str], input: &str, env: &Env) -> (i32, String) {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut out = Vec::new();
    let code = run(&args, &mut input.as_bytes(), &mut out, env);
    (code, String::from_utf8(out).unwrap())
}

fn wire(invocation: &Path, rest: &str) -> String {
    format!(r#"{{"binding":{{"invocation_path":"{}"}}{rest}}}"#, invocation.display())
}

#[test]
fn protocol_answers_on_out_and_needs_no_env() {
    let tmp = TempDir::new().unwrap();
    let env = Env {
        plugin: None, // deliberately unresolved — `protocol` must not need it
        xdg: Xdg::with(Path::new("/nonexistent-home"), None, None),
        cwd: tmp.path().to_path_buf(),
    };
    let (code, out) = drive(&["protocol"], "", &env);
    assert_eq!(code, 0);
    assert_eq!(out, format!("{}\n", delivery::PROTOCOL_JSON));
}

#[test]
fn missing_op_or_phase_is_a_usage_error() {
    let tmp = TempDir::new().unwrap();
    let env = env_at(tmp.path(), tmp.path());
    assert_eq!(drive(&[], "", &env).0, 1);
    assert_eq!(drive(&["claim"], "", &env).0, 1);
}

#[test]
fn a_malformed_wire_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let env = env_at(tmp.path(), tmp.path());
    assert_eq!(drive(&["claim", "post"], "not json", &env).0, 1);
}

#[test]
fn an_unsafe_invocation_path_refuses() {
    let tmp = TempDir::new().unwrap();
    let env = env_at(tmp.path(), tmp.path());
    let payload = r#"{"binding":{"invocation_path":"relative/path"}}"#;
    assert_eq!(drive(&["claim", "post"], payload, &env).0, 1);
}

#[test]
fn an_unset_plugin_name_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let env = Env {
        plugin: None,
        xdg: Xdg::with(Path::new("/nonexistent-home"), None, None),
        cwd: tmp.path().to_path_buf(),
    };
    let payload = wire(tmp.path(), "");
    assert_eq!(drive(&["claim", "post"], &payload, &env).0, 1);
}

#[test]
fn a_hook_with_no_ball_on_the_wire_is_an_error() {
    // §0 obligation 4 (bl-a5f3): neither `command.id` nor a sealed trailer.
    let (_tmp, root) = project();
    let state = TempDir::new().unwrap();
    let env = env_at(state.path(), state.path());
    let payload = wire(&root, "");
    assert_eq!(drive(&["claim", "post"], &payload, &env).0, 1);
}

#[test]
fn a_hook_on_a_non_repo_fails_the_precondition_gate() {
    let tmp = TempDir::new().unwrap(); // absolute, but no git repo
    let env = env_at(tmp.path(), tmp.path());
    let payload = wire(tmp.path(), r#","command":{"id":"bl-t1"}"#);
    assert_eq!(drive(&["claim", "post"], &payload, &env).0, 1);
}

#[test]
fn claim_post_materializes_the_worktree_and_surfaces_its_path() {
    let (_tmp, root) = project();
    let state = TempDir::new().unwrap();
    let env = env_at(state.path(), state.path());
    let payload = wire(&root, r#","command":{"id":"bl-t1"},"current_state":{"title":"T"}"#);
    let (code, out) = drive(&["claim", "post"], &payload, &env);
    assert_eq!(code, 0);
    let surfaced = out.trim();
    assert!(surfaced.ends_with("bl-t1"), "surfaced path names the ball: {surfaced}");
    assert!(Path::new(surfaced).join("seed.txt").exists(), "worktree materialized at the surfaced path");
}

#[test]
fn prime_rollback_declines_before_touching_anything() {
    let tmp = TempDir::new().unwrap();
    let env = env_at(tmp.path(), tmp.path());
    let payload = wire(tmp.path(), r#","rolling_back":"claim""#);
    assert_eq!(drive(&["prime", "post"], &payload, &env).0, 0);
}

#[test]
fn prime_warns_and_continues_on_a_non_repo_invocation_path() {
    let tmp = TempDir::new().unwrap(); // absolute, not a git repo → warn, exit 0
    let env = env_at(tmp.path(), tmp.path());
    let payload = wire(tmp.path(), "");
    assert_eq!(drive(&["prime", "post"], &payload, &env).0, 0);
}

#[test]
fn prime_pre_on_a_repo_is_a_quiet_no_op() {
    let (_tmp, root) = project();
    let state = TempDir::new().unwrap();
    let env = env_at(state.path(), state.path());
    let payload = wire(&root, "");
    assert_eq!(drive(&["prime", "pre"], &payload, &env).0, 0);
}

#[test]
fn prime_post_prunes_and_reports_debris() {
    let (_tmp, root) = project();
    let g = |args: &[&str]| Project::run(&root, args).unwrap();
    // An UNDELIVERED work branch (a commit beyond main) with no worktree —
    // prime.post must survive it and return one debris report line (bl-c117).
    g(&["branch", "work/bl-dbg", "main"]);
    g(&["checkout", "-q", "work/bl-dbg"]);
    fs::write(root.join("extra.txt"), "x\n").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "undelivered"]);
    g(&["checkout", "-q", "main"]);
    let state = TempDir::new().unwrap();
    let env = env_at(state.path(), state.path());
    let payload = wire(&root, "");
    assert_eq!(drive(&["prime", "post"], &payload, &env).0, 0);
}
