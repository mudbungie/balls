//! The shared §8/§14 engine-test harness: fakes for the three seams
//! ([`Anvil`], [`BaseChange`], [`Plugins`]) journal into one shared log so a
//! single sequence assertion proves both WHAT ran and the ORDER. Used by the
//! engine, diffless, narration, and seal-validation test modules.

use super::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub(crate) type Journal = Rc<RefCell<Vec<String>>>;

pub(crate) fn journal() -> Journal {
    Rc::new(RefCell::new(Vec::new()))
}

/// A throwaway log sink for the engine harness: an unwritable path (records are
/// best-effort, so the open fails harmlessly) at the `Error` threshold so the
/// info-level begin/seal records stay quiet — the call sites still execute, and
/// `log_tests` covers the record internals. The engine's logging is tested as
/// behaviour here only through it not perturbing the journaled op sequence.
pub(crate) fn test_log() -> crate::log::Log {
    crate::log::Log::new(Path::new("/nonexistent-balls-test/log").into(), crate::log::Level::Error, Verb::Close, || 0)
}

pub(crate) fn ioerr(what: &str) -> io::Error {
    io::Error::other(what.to_string())
}

pub(crate) fn plugin(name: &str) -> PluginRef {
    PluginRef { name: name.to_string(), bin: None }
}

/// An [`Anvil`] that journals each act, fails the named one, and captures the
/// sealed commit message so a test can assert the §5 trailer landed.
/// `changed_files` is what its `changed` reports — the seal-validation read
/// (bl-528c); it is NOT journaled, a quiet read like the fake's `head`.
pub(crate) struct FakeAnvil {
    pub(crate) j: Journal,
    pub(crate) fail: Option<&'static str>,
    pub(crate) sealed_msg: RefCell<Option<String>>,
    pub(crate) heads: RefCell<u32>,
    pub(crate) changed_files: Vec<String>,
}

impl FakeAnvil {
    pub(crate) fn new(j: Journal, fail: Option<&'static str>) -> Self {
        Self { j, fail, sealed_msg: RefCell::new(None), heads: RefCell::new(0), changed_files: Vec::new() }
    }
    pub(crate) fn log(&self, s: String) {
        self.j.borrow_mut().push(s);
    }
    pub(crate) fn gate(&self, m: &'static str) -> io::Result<()> {
        if self.fail == Some(m) { Err(ioerr(m)) } else { Ok(()) }
    }
}

impl Anvil for FakeAnvil {
    fn head(&self) -> io::Result<String> {
        self.log("head".into());
        self.gate("head")?;
        // A distinct tip per read so a diffless before/after pair differs (T0,
        // T1, …); the seal path reads head once, so it stays "T0".
        let n = *self.heads.borrow();
        *self.heads.borrow_mut() += 1;
        Ok(format!("T{n}"))
    }
    fn open(&self, _dir: &Path) -> io::Result<()> {
        self.log("open".into());
        self.gate("open")
    }
    fn changed(&self, _dir: &Path) -> io::Result<Vec<String>> {
        self.gate("changed")?;
        Ok(self.changed_files.clone())
    }
    fn seal(&self, _dir: &Path, message: &str) -> io::Result<String> {
        self.log("seal".into());
        self.gate("seal")?;
        *self.sealed_msg.borrow_mut() = Some(message.to_string());
        Ok("C1".into())
    }
    fn unseal(&self, sha: &str) -> io::Result<()> {
        self.log(format!("unseal:{sha}"));
        self.gate("unseal")
    }
    fn close(&self, _dir: &Path) -> io::Result<()> {
        self.log("close".into());
        self.gate("close")
    }
}

/// A [`Plugins`] that journals every run/rollback and fails the named plugin.
/// `seen` captures the `Sealed` commit each call observed so a test can prove the
/// §7 post facts cross the seam (`None` on every `pre` call).
pub(crate) struct FakePlugins {
    pub(crate) j: Journal,
    pub(crate) fail: Option<&'static str>,
    pub(crate) seen: RefCell<Vec<(String, Option<String>)>>,
}

impl FakePlugins {
    pub(crate) fn new(j: Journal, fail: Option<&'static str>) -> Self {
        Self { j, fail, seen: RefCell::new(Vec::new()) }
    }
}

impl Plugins for FakePlugins {
    fn run(&self, p: &PluginRef, _op: Verb, phase: Phase, _dir: &Path, sealed: Option<&Sealed>) -> io::Result<()> {
        self.j.borrow_mut().push(format!("run:{}:{}", p.name, phase.token()));
        self.seen.borrow_mut().push((p.name.clone(), sealed.map(|s| s.commit.to_string())));
        if self.fail == Some(p.name.as_str()) { Err(ioerr(&p.name)) } else { Ok(()) }
    }
    fn rollback(&self, p: &PluginRef, _op: Verb, phase: Phase, _dir: &Path, sealed: Option<&Sealed>) {
        self.j.borrow_mut().push(format!("rollback:{}:{}", p.name, phase.token()));
        self.seen.borrow_mut().push((format!("rb:{}", p.name), sealed.map(|s| s.commit.to_string())));
    }
}

/// A [`BaseChange`] that journals stage/finalize and fails on demand.
pub(crate) struct FakeBase {
    pub(crate) j: Journal,
    pub(crate) fail_stage: bool,
    pub(crate) fail_finalize: bool,
}

impl BaseChange for FakeBase {
    fn stage(&self, _dir: &Path) -> io::Result<()> {
        self.j.borrow_mut().push("stage".into());
        if self.fail_stage { Err(ioerr("stage")) } else { Ok(()) }
    }
    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        self.j.borrow_mut().push("finalize".into());
        if self.fail_finalize { Err(ioerr("finalize")) } else { Ok("MSG".into()) }
    }
}
