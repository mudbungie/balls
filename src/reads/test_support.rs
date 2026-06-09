//! Shared fixtures for the read-verb tests: build a [`Catalog`] from in-memory
//! balls without a git checkout, and a throwaway git store carrying real
//! create/retire history for the dead-ball reconstruction tests (§9).

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

use super::Catalog;
use crate::edge::Edge;
use crate::layout::Xdg;
use crate::log::{self, Level, Log};
use crate::registry::Registry;
use crate::task::{Blocker, On, Task};
use crate::verb::Verb;

/// An [`Edge`] rooted in `tmp`: XDG state under `tmp/state`, the project at
/// `tmp/proj`, no colour, top-level depth unless overridden.
pub(crate) fn edge(tmp: &Path, depth: u32) -> Edge {
    Edge {
        xdg: Xdg::with(&tmp.join("home"), None, Some(tmp.join("state").to_str().unwrap())),
        invocation_path: tmp.join("proj"),
        default_actor: "me".into(),
        depth,
        exe_dir: None,
        color: false,
        log_level: None,
    }
}

/// The landing dir for `edge`'s project, with `config/plugins.toml` written.
pub(crate) fn landing_with(edge: &Edge, plugins_toml: &str) -> PathBuf {
    let landing = edge.xdg.clone_dir(&edge.invocation_path).landing();
    fs::create_dir_all(landing.join("config")).unwrap();
    fs::write(landing.join("config").join("plugins.toml"), plugins_toml).unwrap();
    landing
}

/// Drop an executable `script` named `name` in `tmp` and bind it on `landing`.
pub(crate) fn bind_script(tmp: &Path, landing: &Path, name: &str, script: &str) {
    let bin = tmp.join(name);
    fs::write(&bin, script).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    Registry::at(landing).bind(name, &bin).unwrap();
}

/// A [`Log`] sink at `tmp/oplog` with the given threshold — read its records
/// back with [`log_lines`].
pub(crate) fn log_at(tmp: &Path, level: Level, verb: Verb) -> Log {
    Log::new(tmp.join("oplog"), level, verb, log::wall)
}

/// The records [`log_at`]'s sink wrote — `""` when nothing was emitted.
pub(crate) fn log_lines(tmp: &Path) -> String {
    fs::read_to_string(tmp.join("oplog")).unwrap_or_default()
}

/// The op-log contents for `edge`'s clone — `""` when no record was emitted.
pub(crate) fn op_log(edge: &Edge) -> String {
    fs::read_to_string(edge.xdg.clone_dir(&edge.invocation_path).op_log()).unwrap_or_default()
}

/// A minimal ready ball: a title and a timestamp, everything else default.
pub(crate) fn task(title: &str, created: i64) -> Task {
    Task { title: title.into(), created, updated: created, ..Default::default() }
}

/// A `{id, on}` blocker edge.
pub(crate) fn blocker(id: &str, on: On) -> Blocker {
    Blocker { id: id.into(), on }
}

/// Write each `(id, task)` to a fresh store tempdir and load the catalog. The
/// tempdir may drop after — [`Catalog::load`] reads every file into memory.
pub(crate) fn catalog(tasks: &[(&str, Task)]) -> Catalog {
    let tmp = TempDir::new().unwrap();
    for (id, t) in tasks {
        crate::taskfile::write_task(tmp.path(), id, t).unwrap();
    }
    Catalog::load(tmp.path()).unwrap()
}

/// A throwaway git store carrying real `balls/tasks`-style history — the fixture
/// the dead-ball reconstruction tests walk (`show` fallthrough, `list --status closed`).
/// Commits go through the §5 trailer protocol so `bl-op:` reads back as the
/// retirement; commit dates are pinned so a deletion's `retired_at` is exact.
pub(crate) struct GitStore {
    _tmp: Option<TempDir>,
    dir: PathBuf,
}

/// A git store in a fresh tempdir (kept alive by the returned value).
pub(crate) fn git_store() -> GitStore {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    let store = GitStore { _tmp: Some(tmp), dir };
    store.init();
    store
}

/// A git store initialised in an existing `dir` (e.g. an [`crate::edge::Edge`]'s
/// derived store path) — the caller owns the directory's lifetime.
pub(crate) fn git_store_at(dir: &Path) -> GitStore {
    std::fs::create_dir_all(dir).unwrap();
    let store = GitStore { _tmp: None, dir: dir.to_path_buf() };
    store.init();
    store
}

impl GitStore {
    /// The store checkout path — what the read verbs run git against.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    fn init(&self) {
        let git = |args: &[&str]| crate::git::run(&self.dir, args, None).unwrap();
        git(&["init", "-q"]);
        git(&["config", "user.name", "test"]);
        git(&["config", "user.email", "t@x"]);
    }

    /// Commit `tasks/<id>.md` into being as a `create` (born at unix `at`).
    pub(crate) fn create(&self, id: &str, task: &Task, at: i64) -> &Self {
        crate::taskfile::write_task(&self.dir, id, task).unwrap();
        self.commit(id, "create", at);
        self
    }

    /// Commit raw (possibly malformed) `content` as `tasks/<id>.md` — for the
    /// corrupt-history reconstruction path, where parse must surface an error.
    pub(crate) fn create_raw(&self, id: &str, content: &str, at: i64) -> &Self {
        let path = crate::taskfile::task_path(&self.dir, id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        self.commit(id, "create", at);
        self
    }

    /// Delete `tasks/<id>.md` as `op` (`close`, or a legacy `drop`) at unix `at` — the
    /// retirement commit the recency walk reads `bl-op:` and `retired_at` from.
    pub(crate) fn retire(&self, id: &str, op: &str, at: i64) -> &Self {
        std::fs::remove_file(crate::taskfile::task_path(&self.dir, id)).unwrap();
        self.commit(id, op, at);
        self
    }

    fn commit(&self, id: &str, op: &str, at: i64) {
        crate::git::run(&self.dir, &["add", "-A"], None).unwrap();
        let msg = format!("{op} {id}\n\nbl-protocol: 1\nbl-op: {op}\nbl-id: {id}\nbl-actor: t\n");
        let date = format!("@{at} +0000");
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["commit", "-q", "-F", "-"])
            .env("GIT_COMMITTER_DATE", &date)
            .env("GIT_AUTHOR_DATE", &date)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(msg.as_bytes()).unwrap();
        assert!(child.wait().unwrap().success());
    }
}
