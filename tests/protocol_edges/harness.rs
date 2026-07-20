//! Shared isolated substrate for the §6/§7/§8/§14 plugin-protocol edge tests.
//!
//! Each test stands up its own tempdir (own HOME/XDG_STATE, throwaway project,
//! a stealth landing) and drives the freshly-built `bl` against fake shell-script
//! plugins bound into the landing's `config/plugins/bin/`. Nothing touches the
//! real store. The `BALLS_*` recursion bookkeeping is scrubbed so a top-level
//! `bl` here starts at depth 0 (this test itself runs inside a `bl close` gate).

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// Every fake plugin answers `<bin> protocol` (so `bl install`/validation would
/// be happy) and otherwise runs the per-plugin `body`.
const HEAD: &str = "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{\"protocol\":[1],\"ops\":[\"create\",\"list\",\"show\"]}'; exit 0; fi\n";

/// One wired, isolated substrate.
pub(crate) struct Env {
    pub home: PathBuf,
    pub state: PathBuf,
    pub project: PathBuf,
    pub bins: PathBuf,
    _tmp: TempDir,
}

/// A primed stealth substrate: no remote, so `prime` founds a stealth landing and
/// runs the shipped tracker/delivery chain end to end (dispatch.rs pattern).
pub(crate) fn setup() -> Env {
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("home"), tmp.path().join("state"));
    let (project, bins) = (tmp.path().join("proj"), tmp.path().join("bins"));
    for d in [&home, &project, &bins] {
        fs::create_dir_all(d).unwrap();
    }
    let e = Env { home, state, project, bins, _tmp: tmp };
    let out = e.bl(&["prime"]);
    assert!(out.status.success(), "prime failed: {}", String::from_utf8_lossy(&out.stderr));
    e
}

impl Env {
    /// A configured (not-yet-run) `bl` command in this substrate.
    pub(crate) fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::cargo_bin("bl").unwrap();
        c.current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BALLS_PLUGIN_DEPTH")
            .env_remove("BALLS_PLUGIN_NAME")
            .args(args);
        c
    }

    /// Run `bl <args>`, returning the raw output.
    pub(crate) fn bl(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    /// Run `bl`, assert success, return trimmed stdout.
    pub(crate) fn ok(&self, args: &[&str]) -> String {
        let out = self.bl(args);
        assert!(out.status.success(), "bl {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// This substrate's clone bundle (the same layout `bl` resolves).
    fn clone_dir(&self) -> balls::layout::CloneDir {
        balls::layout::Xdg::with(&self.home, None, Some(&self.state.to_string_lossy())).clone_dir(&self.project)
    }

    /// The landing's local binding store `config/plugins/bin/`.
    fn bin_dir(&self) -> PathBuf {
        self.clone_dir().landing().join("config").join("plugins").join("bin")
    }

    /// The unified per-clone op log — `clones/<enc>/log`.
    pub(crate) fn op_log(&self) -> PathBuf {
        self.clone_dir().op_log()
    }

    /// Every JSON-lines record currently in the op log.
    pub(crate) fn log_records(&self) -> Vec<Value> {
        let body = fs::read_to_string(self.op_log()).unwrap_or_default();
        body.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    /// Bind `name` → `target` as the local (gitignored) `bin/<name>` symlink.
    pub(crate) fn bind(&self, name: &str, target: &Path) {
        let dir = self.bin_dir();
        fs::create_dir_all(&dir).unwrap();
        let link = dir.join(name);
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    /// Write an executable fake plugin whose non-`protocol` body is `body`.
    pub(crate) fn write_plugin(&self, name: &str, body: &str) -> PathBuf {
        let path = self.bins.join(name);
        let mut script = String::from(HEAD);
        script.push_str(body);
        script.push('\n');
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// A plugin that records each invocation to `marker` — `FWD <name>` on a
    /// forward call, `RB <name> <tag>` on a rollback (reading the §7
    /// `rolling_back` tag off stdin) — then exits `code`. Bound under `name`.
    pub(crate) fn stamp_plugin(&self, name: &str, marker: &Path, code: i32) {
        let body = STAMP
            .replace("__NAME__", name)
            .replace("__MARKER__", &marker.display().to_string())
            .replace("__EXIT__", &code.to_string());
        let path = self.write_plugin(name, &body);
        self.bind(name, &path);
    }
}

/// The stamping body (raw so the sed backslashes survive verbatim).
const STAMP: &str = r#"payload=$(cat)
case "$payload" in
  *rolling_back*) tag=$(printf '%s' "$payload" | sed -n 's/.*"rolling_back":"\([^"]*\)".*/\1/p'); echo "RB __NAME__ $tag" >> __MARKER__ ;;
  *) echo "FWD __NAME__" >> __MARKER__ ;;
esac
exit __EXIT__"#;
