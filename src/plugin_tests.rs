//! §6 dispatch tests: drive [`Subprocess`] against throwaway shell-script
//! plugins so the real spawn path — env, stdin payload, cwd, stderr capture,
//! exit-code-to-abort, the recursion guard, and `protocol` self-describe — is
//! exercised end to end.

use super::*;
use crate::lifecycle::{Plugins, Sealed};
use crate::log::{Level, Log};
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::verb::Verb;
use crate::wire::{Binding, Command, OpContext};

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;

/// Write an executable `#!/bin/sh` plugin into `dir` and return its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A plugin that records its cwd-relative env + stdin and logs to stderr.
const RECORDER: &str = "env > env.txt\ncat > stdin.txt\nprintf 'log from %s\\n' \"$BALLS_PLUGIN_NAME\" >&2\n";

fn pref(name: &str, bin: Option<PathBuf>) -> PluginRef {
    PluginRef { name: name.into(), bin }
}

fn ctx() -> OpContext {
    OpContext {
        actor: "me@example.com".into(),
        binding: Binding {
            remote: None,
            tasks_branch: "balls/tasks".into(),
            store: "/store".into(),
            landing: "/landing".into(),
            invocation_path: "/proj".into(),
        },
        command: Some(Command { op: "close".into(), body_change: None }),
        before: None,
    }
}

/// A frozen clock so a record's `ts` is stable.
fn clk() -> i64 {
    0
}

/// A workspace: a bin dir for scripts, a cwd for the plugin, and the unified op
/// log every dispatcher shares (threshold `Debug` so nothing is filtered out).
struct Env {
    home: TempDir,
    log: Log,
}

impl Env {
    fn new() -> Self {
        let home = TempDir::new().unwrap();
        for sub in ["bin", "cwd"] {
            fs::create_dir(home.path().join(sub)).unwrap();
        }
        let log = Log::new(home.path().join("log"), Level::Debug, Verb::Close, clk);
        Self { home, log }
    }
    fn at(&self, sub: &str) -> PathBuf {
        self.home.path().join(sub)
    }
    /// The unified op log path; its records are JSON-lines.
    fn log_path(&self) -> PathBuf {
        self.home.path().join("log")
    }
    fn dispatcher(&self, depth: u32) -> Subprocess<'_> {
        Subprocess::new(ctx(), &self.log, depth)
    }
}

#[test]
fn run_delivers_the_env_stdin_and_cwd() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "rec", RECORDER);
    e.dispatcher(0)
        .run(&pref("tracker", Some(bin)), Verb::Close, Phase::Pre, &e.at("cwd"), None)
        .unwrap();
    let env = fs::read_to_string(e.at("cwd").join("env.txt")).unwrap();
    assert!(env.contains("BALLS_PROTOCOL=1"));
    assert!(env.contains("BALLS_PLUGIN_NAME=tracker"));
    assert!(env.contains("BALLS_PLUGIN_DEPTH=1")); // top level 0, child +1
    let stdin = fs::read_to_string(e.at("cwd").join("stdin.txt")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdin).unwrap();
    assert_eq!(v["op"], "close");
    assert_eq!(v["phase"], "pre");
    assert_eq!(v["plugin_name"], "tracker");
}

#[test]
fn run_envelopes_stderr_into_the_unified_log() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "rec", RECORDER);
    e.dispatcher(0)
        .run(&pref("tracker", Some(bin)), Verb::Close, Phase::Pre, &e.at("cwd"), None)
        .unwrap();
    let log = fs::read_to_string(e.log_path()).unwrap();
    // Core logs the `invoke` first (src=core), then the plugin's stderr line is
    // enveloped (src=tracker, lvl=info), each its own JSON object.
    let recs: Vec<serde_json::Value> = log.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(recs[0]["src"], "core");
    assert_eq!(recs[0]["msg"], "invoke tracker");
    let envelope = recs.iter().find(|r| r["src"] == "tracker").unwrap();
    assert_eq!(envelope["lvl"], "info");
    assert_eq!(envelope["phase"], "pre");
    assert_eq!(envelope["msg"], "log from tracker");
}

#[test]
fn a_post_run_carries_the_sealed_commit_and_parsed_metadata() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "rec", RECORDER);
    let sealed = Sealed { commit: "C1", previous_commit: "T0", message: Some("subj\n\nbl-id: bl-9\n") };
    e.dispatcher(0)
        .run(&pref("tracker", Some(bin)), Verb::Close, Phase::Post, &e.at("cwd"), Some(&sealed))
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(e.at("cwd").join("stdin.txt")).unwrap()).unwrap();
    assert_eq!(v["commit"], "C1");
    assert_eq!(v["previous_commit"], "T0");
    assert_eq!(v["metadata"]["bl-id"][0], "bl-9");
}

#[test]
fn a_diffless_post_run_carries_the_commit_pair_but_no_metadata() {
    // §13: a messageless `Sealed` (sync/prime moved the tip, sealed no §5
    // message) wires previous_commit/commit through but omits `metadata`.
    let e = Env::new();
    let bin = script(&e.at("bin"), "rec", RECORDER);
    let sealed = Sealed { commit: "T1", previous_commit: "T0", message: None };
    e.dispatcher(0)
        .run(&pref("tracker", Some(bin)), Verb::Sync, Phase::Post, &e.at("cwd"), Some(&sealed))
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(e.at("cwd").join("stdin.txt")).unwrap()).unwrap();
    assert_eq!(v["commit"], "T1");
    assert_eq!(v["previous_commit"], "T0");
    assert!(v.get("metadata").is_none(), "diffless post must omit metadata");
}

#[test]
fn a_nonzero_exit_aborts_the_op() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "fail", "cat >/dev/null\nexit 7\n");
    let err = e
        .dispatcher(0)
        .run(&pref("tracker", Some(bin)), Verb::Close, Phase::Pre, &e.at("cwd"), None)
        .unwrap_err();
    assert!(err.to_string().contains("tracker aborted the op"));
    // Core records the failure locus at `error` so it survives any threshold (§6).
    let log = fs::read_to_string(e.log_path()).unwrap();
    let err_rec = log.lines().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()).find(|r| r["lvl"] == "error").unwrap();
    assert_eq!(err_rec["src"], "core");
    assert!(err_rec["msg"].as_str().unwrap().contains("tracker aborted the op"));
}

#[test]
fn a_missing_bin_errors_at_the_op_and_names_bl_install() {
    let e = Env::new();
    let err = e
        .dispatcher(0)
        .run(&pref("ghost", None), Verb::Close, Phase::Pre, &e.at("cwd"), None)
        .unwrap_err();
    assert!(err.to_string().contains("ghost referenced but bin/ghost missing — run bl install"));
}

#[test]
fn a_missing_binary_path_surfaces_the_spawn_error() {
    let e = Env::new();
    let bin = e.at("bin").join("does-not-exist");
    let err = e
        .dispatcher(0)
        .run(&pref("gone", Some(bin)), Verb::Close, Phase::Pre, &e.at("cwd"), None)
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

#[test]
fn a_busy_binary_retries_then_surfaces_the_error() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "rec", RECORDER);
    // Hold the binary open for writing for the whole call: every exec sees
    // ETXTBSY, so retry_busy exhausts its budget and surfaces the busy error.
    let _held = fs::OpenOptions::new().write(true).open(&bin).unwrap();
    let err = e
        .dispatcher(0)
        .run(&pref("tracker", Some(bin)), Verb::Close, Phase::Pre, &e.at("cwd"), None)
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::ExecutableFileBusy);
}

#[test]
fn the_depth_cap_aborts_the_op() {
    let e = Env::new();
    // A plugin that records its stdin if it runs — proves the cap aborts BEFORE
    // the spawn (bl-7110: fail, not silent), never executing the plugin.
    let bin = script(&e.at("bin"), "rec", RECORDER);
    let plugin = pref("tracker", Some(bin));
    let d = e.dispatcher(DEPTH_CAP);
    let err = d.run(&plugin, Verb::Close, Phase::Pre, &e.at("cwd"), None).unwrap_err();
    assert!(err.to_string().contains("depth cap"), "the abort names the cap");
    // rollback at the cap cannot spawn — best-effort no-op.
    d.rollback(&plugin, Verb::Close, Phase::Pre, &e.at("cwd"), None);
    assert!(!e.at("cwd").join("stdin.txt").exists(), "the plugin never spawned at the cap");
    // the abort emitted an `error` record so the runaway surfaces (§6).
    let log = fs::read_to_string(e.log_path()).unwrap();
    assert!(log.contains("depth cap") && log.contains("\"lvl\":\"error\""));
}

#[test]
fn rollback_tags_the_payload_and_ignores_the_exit() {
    let e = Env::new();
    // Records its stdin, then exits non-zero — rollback must swallow the exit.
    let bin = script(&e.at("bin"), "rec", &format!("{RECORDER}exit 3\n"));
    let sealed = Sealed { commit: "C1", previous_commit: "T0", message: Some("s\n\nbl-id: bl-1\n") };
    e.dispatcher(0)
        .rollback(&pref("tracker", Some(bin)), Verb::Close, Phase::Post, &e.at("cwd"), Some(&sealed));
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(e.at("cwd").join("stdin.txt")).unwrap()).unwrap();
    assert_eq!(v["rolling_back"], "post");
    assert_eq!(v["commit"], "C1");
    assert!(fs::read_to_string(e.log_path()).unwrap().contains("rollback failed (exit status: 3) — its close.post side effects may not be unwound"));
}

const PROTO: &str =
    "if [ \"$1\" = protocol ]; then printf '%s' '{\"protocol\":1,\"ops\":[\"close\",\"claim\"]}'; exit 0; fi\n";

#[test]
fn describe_reads_a_scalar_protocol_version() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", PROTO);
    let p = describe(&bin).unwrap();
    assert_eq!(p.protocol, [1]);
    assert!(p.handles(Verb::Close));
    assert!(!p.handles(Verb::Sync));
    assert!(p.speaks(1));
}

#[test]
fn describe_reads_a_list_protocol_version() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", "printf '%s' '{\"protocol\":[1,2],\"ops\":[]}'\n");
    let p = describe(&bin).unwrap();
    assert_eq!(p.protocol, [1, 2]);
    assert!(p.speaks(2));
    assert!(!p.speaks(9));
    assert!(!p.handles(Verb::Close));
}

#[test]
fn describe_errors_on_a_nonzero_exit() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", "exit 1\n");
    let err = describe(&bin).unwrap_err();
    assert!(err.to_string().contains("self-describe exited"));
}

#[test]
fn describe_errors_on_unparseable_output() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", "printf 'not json'\n");
    assert!(describe(&bin).is_err());
}

#[test]
fn describe_errors_when_the_binary_is_missing() {
    let e = Env::new();
    assert!(describe(&e.at("bin").join("nope")).is_err());
}

#[test]
fn capped_lines_splits_lines_and_trims_newlines() {
    // A newline-terminated stream and a final un-terminated blob both surface,
    // each with its trailing '\n' trimmed.
    let mut got = Vec::new();
    capped_lines(&b"alpha\nbeta\ngamma"[..], RELAY_LINE_MAX, |l| got.push(l.to_string()));
    assert_eq!(got, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn capped_lines_bounds_a_no_newline_flood() {
    // 10 KiB with no newline, cap 4 bytes: it is flushed in <=cap pieces rather
    // than buffered whole — the bl-2d6d OOM guard. Reassembled, no byte is lost.
    let flood = "x".repeat(10_240);
    let mut pieces = Vec::new();
    capped_lines(flood.as_bytes(), 4, |l| pieces.push(l.to_string()));
    assert!(pieces.iter().all(|p| p.len() <= 4), "every piece stays within the cap");
    assert_eq!(pieces.concat(), flood, "no byte dropped");
}
