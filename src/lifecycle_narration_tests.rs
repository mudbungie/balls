//! bl-cf93: the no-op seal vs `-m` narration. A converged seal (nothing
//! staged, tip unchanged) is the §13 idempotent default — but the §5 free
//! body exists ONLY in a sealed commit, so an op that carries one must abort
//! (and unwind) instead of silently dropping the note. Shares the §8 engine
//! harness ([`super`]: the fakes + `journal`/`plugin` helpers).

use super::*;

/// An [`Anvil`] whose seal CONVERGES: it returns the same tip `head` reported,
/// modelling git.rs's "nothing staged ⇒ the existing tip" path.
struct ConvergingAnvil {
    j: Journal,
}

impl Anvil for ConvergingAnvil {
    fn head(&self) -> io::Result<String> {
        Ok("T0".into())
    }
    fn open(&self, _dir: &Path) -> io::Result<()> {
        self.j.borrow_mut().push("open".into());
        Ok(())
    }
    fn changed(&self, _dir: &Path) -> io::Result<Vec<String>> {
        Ok(Vec::new()) // nothing staged — the converging case
    }
    fn seal(&self, _dir: &Path, _message: &str) -> io::Result<String> {
        self.j.borrow_mut().push("seal".into());
        Ok("T0".into())
    }
    fn unseal(&self, sha: &str) -> io::Result<()> {
        self.j.borrow_mut().push(format!("unseal:{sha}"));
        Ok(())
    }
    fn close(&self, _dir: &Path) -> io::Result<()> {
        self.j.borrow_mut().push("close".into());
        Ok(())
    }
}

/// A narrated base staging nothing — the pure-note `update <id> -m NOTE`.
struct NarratedBase;

impl BaseChange for NarratedBase {
    fn stage(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }
    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        Ok("MSG".into())
    }
    fn narrated(&self) -> bool {
        true
    }
}

/// The fake's `unseal` exists only to satisfy [`Anvil`] — the refusal test
/// asserts the engine never reaches it (nothing sealed, nothing to un-seal),
/// so its journaling arm is covered by this direct call alone.
#[test]
fn the_converging_fakes_unseal_only_journals() {
    let jrn = journal();
    let anvil = ConvergingAnvil { j: jrn.clone() };
    anvil.unseal("X").unwrap();
    assert_eq!(*jrn.borrow(), ["unseal:X"]);
}

#[test]
fn a_narrated_op_refuses_the_noop_seal_and_unwinds() {
    let jrn = journal();
    let anvil = ConvergingAnvil { j: jrn.clone() };
    let plugins = FakePlugins::new(jrn.clone(), None);
    let pre = [plugin("p")];
    let post = [plugin("q")];
    let err = Engine::new(&anvil, &plugins, &test_log())
        .seal(&NarratedBase, Verb::Update, Path::new("/c"), &pre, &post)
        .unwrap_err();
    assert!(err.to_string().contains("-m note"), "names the dropped note: {err}");
    assert!(format!("{err:?}").contains("Narration"));
    // Pre rolled back, post never ran, and nothing un-seals — the tip never
    // moved, so the unwind only discards the change worktree.
    assert_eq!(*jrn.borrow(), ["open", "run:p:pre", "seal", "rollback:p:pre", "close"]);
}

#[test]
fn an_unnarrated_noop_seal_still_converges_quietly() {
    // §13 idempotence is untouched: no `-m`, so converging on the tip is the
    // correct silent no-op and `post` reacts to the unmoved tip.
    let jrn = journal();
    let anvil = ConvergingAnvil { j: jrn.clone() };
    let plugins = FakePlugins::new(jrn.clone(), None);
    let base = FakeBase { j: jrn.clone(), fail_stage: false, fail_finalize: false };
    let post = [plugin("q")];
    let sha = Engine::new(&anvil, &plugins, &test_log())
        .seal(&base, Verb::Update, Path::new("/c"), &[], &post)
        .unwrap();
    assert_eq!(sha, "T0");
    assert_eq!(*plugins.seen.borrow(), [("q".into(), Some("T0".into()))]);
}
