//! §8 anvil git plumbing — the change worktree and the SEAL.
//!
//! [`Anvil`] is the narrow seam the op lifecycle ([`crate::lifecycle`])
//! drives: make the change worktree off the anvil, SEAL (commit + integrate
//! the worktree onto the anvil, atomically), un-seal a post-abort (§14
//! tier-1), and tear the worktree down. [`Git`] is the real implementation,
//! shelling out to git against one checkout; the lifecycle is
//! unit-tested against a fake while [`Git`] is tested here on throwaway repos.
//!
//! Topology: the checkout has the anvil branch at commit `T`.
//! A change worktree is a DETACHED worktree created at `T`; balls + plugins
//! edit it, then SEAL commits it to `C` (parent `T`) and fast-forwards the
//! anvil onto `C` in one act. A post-seal abort `git reset --hard`s the
//! anvil back to `T` — local and reversible, because core never pushes (§14).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The git acts the §8 seal needs, behind a seam so the lifecycle can be tested
/// without a real repo. Each method is one atomic git act on the anvil.
pub trait Anvil {
    /// The anvil tip — captured before the seal so a post-abort can un-seal.
    fn head(&self) -> io::Result<String>;
    /// (§8.1) Make the change worktree at `dir`, detached at the anvil tip.
    ///
    /// **INVARIANT — ONE HEAD PER OP (bl-057a): the tip forked here is the same
    /// commit the base change read the live store from.** `create` reads the live
    /// id set off the anvil CHECKOUT before the lifecycle opens the worktree
    /// ([`crate::change_create::Create::existing`]) and recovers the minted ball at
    /// finalize as the set difference against the WORKTREE — two reads of one
    /// store, sound only while both name the same commit. They agree by
    /// construction today: the seal's `merge --ff-only` advances the checkout
    /// itself, so the checkout is never behind its own branch, and nothing
    /// advances the store branch by plumbing behind it. Break that — advance the
    /// branch without moving the checkout — and the difference silently counts
    /// balls that were merely unseen: `create` dies at finalize with "expected
    /// exactly one new task file, found N", a symptom with no route back to its
    /// cause. Deliberately NOT a `debug_assert` here: `open` sees only the
    /// checkout, where HEAD equals HEAD vacuously; the invariant spans the read in
    /// [`crate::mutate_author`] and this fork, which is exactly why it is written
    /// down rather than checked.
    fn open(&self, dir: &Path) -> io::Result<()>;
    /// (§8.3) The paths the change worktree `dir` touched relative to the anvil
    /// tip — what the seal-validation read (bl-528c) parses before committing.
    /// Renames report as delete+add (`--no-renames`), so every entry is one path.
    fn changed(&self, dir: &Path) -> io::Result<Vec<String>>;
    /// (§8.3) SEAL: commit everything in `dir` with `message`, then fast-forward
    /// the anvil onto it — atomically. Returns the sealed commit sha. A change
    /// that stages NOTHING (the tree already equals the tip) seals to the
    /// EXISTING tip — no empty commit, so a byte-identical re-run of an op
    /// converges instead of erroring (§13 idempotence; `install` of identical
    /// content is the canonical case).
    fn seal(&self, dir: &Path, message: &str) -> io::Result<String>;
    /// (§14 tier-1) Un-seal a post-abort: reset the anvil back to `sha`.
    fn unseal(&self, sha: &str) -> io::Result<()>;
    /// (§8.5 / §14) Remove the change worktree — teardown, or discard on abort.
    fn close(&self, dir: &Path) -> io::Result<()>;
}

/// The real [`Anvil`]: shells out to git against one checkout.
pub struct Git {
    checkout: PathBuf,
    date: Option<i64>,
}

impl Git {
    /// Operate against the anvil checkout rooted at `checkout`. Un-dated: the seal
    /// commit takes git's own clock, as before bl-8b98.
    pub fn at(checkout: &Path) -> Self {
        Self { checkout: checkout.to_path_buf(), date: None }
    }

    /// Pin this anvil's seal commit to the op instant `t` (§8): its
    /// `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` are set from `t` so the store commit
    /// date derives from the SAME instant as the frontmatter and the delivery
    /// squash ([`crate::clock`]), not a third independent clock read.
    #[must_use]
    pub fn dated(mut self, t: i64) -> Self {
        self.date = Some(t);
        self
    }
}

/// Run `git -C <cwd> <args>`, optionally feeding `stdin`, returning stdout. A
/// non-zero exit becomes an [`io::Error`] carrying git's stderr — the one
/// git-invocation site. Shared between the §8 anvil seal ([`Git`]) and the
/// §12/§13 checkout-lifecycle ops ([`crate::substrate`]): both author LOCAL git
/// only — STORE remote talk (sync/push) is the tracker's alone (§0). The ONE
/// core exception is [`crate::adopt`]'s config install-transport: a `fetch` that
/// READS a center's config to copy in (§0 — "config crosses into a landing only
/// by the explicit copy `install` performs"), never a push, never the store.
pub(crate) fn run(cwd: &Path, args: &[&str], stdin: Option<&str>) -> io::Result<String> {
    run_env(cwd, args, stdin, &[])
}

/// [`run`] plus extra environment on the child git — the seam the §8 seal uses to
/// stamp its commit with the op instant (`GIT_*_DATE`, [`crate::clock`]). `env`
/// is empty for every non-commit git act, so those stay byte-identical.
fn run_env(cwd: &Path, args: &[&str], stdin: Option<&str>, env: &[(&'static str, String)]) -> io::Result<String> {
    let mut cmd = crate::safegit::at(cwd);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn()?;
    if let Some(text) = stdin {
        use io::Write;
        child.stdin.take().expect("stdin was configured as a pipe").write_all(text.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(io::Error::other(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// A REJECTED STORE SEAL, in balls' voice (bl-fa89) — the bl-a3bb precedent one
/// layer down ([`crate::delivery_repo_acts::commit_swap`] does the same for the
/// delivery ref).
///
/// The seal's `merge --ff-only` IS the §8 compare-and-swap and a rejection is
/// working correctly: it means the store branch is no longer where this op's
/// commit forks from, so the commit did not integrate and nothing was written.
/// Whichever way git spells the loss — `Not possible to fast-forward`,
/// `cannot lock ref HEAD`, `Your local changes would be overwritten` — the fact
/// and the remedy are identical, and raw git reads as CORRUPTION rather than as
/// the one-line instruction it is. So the whole rejection speaks once, naming
/// what happened, that nothing is damaged, and the §14 converge-on-retry move.
///
/// Deliberately NOT a retry loop in core: the retry is one command, and an
/// in-core loop would hide contention and double the wall-clock of a genuine
/// conflict.
fn contended() -> io::Error {
    io::Error::other(
        "the store moved under this op — a concurrent `bl` won the seal; nothing was written. \
         Re-run the command: it re-reads the moved store and seals onto its new tip",
    )
}

impl Anvil for Git {
    fn head(&self) -> io::Result<String> {
        Ok(run(&self.checkout, &["rev-parse", "HEAD"], None)?.trim().to_string())
    }

    fn open(&self, dir: &Path) -> io::Result<()> {
        run(&self.checkout, &["worktree", "add", "--detach", &dir.to_string_lossy(), "HEAD"], None)?;
        Ok(())
    }

    fn changed(&self, dir: &Path) -> io::Result<Vec<String>> {
        let out = run(dir, &["status", "--porcelain", "--no-renames"], None)?;
        // Each line is `XY <path>`; byte 3 onward is the path.
        Ok(out.lines().filter_map(|l| l.get(3..)).map(str::to_string).collect())
    }

    fn seal(&self, dir: &Path, message: &str) -> io::Result<String> {
        run(dir, &["add", "-A"], None)?;
        // Nothing staged (`diff --cached --quiet` exits 0) ⇒ the no-op seal:
        // the op converges on the existing tip instead of an empty commit.
        if run(dir, &["diff", "--cached", "--quiet"], None).is_ok() {
            return self.head();
        }
        // The one dated git act (§8): the seal commit derives its date from the op
        // instant `T` when set, so it agrees with the frontmatter and the delivery
        // squash by construction. Un-dated ⇒ git's own clock, as before.
        match self.date {
            Some(t) => run_env(dir, &["commit", "-F", "-"], Some(message), &crate::clock::git_date_env(t))?,
            None => run(dir, &["commit", "-F", "-"], Some(message))?,
        };
        let sha = run(dir, &["rev-parse", "HEAD"], None)?.trim().to_string();
        if run(&self.checkout, &["merge", "--ff-only", &sha], None).is_err() {
            // A lost merge (e.g. the ref-lock race two simultaneous claims run,
            // bl-07d6) can strand the loser's tree STAGED in the checkout
            // index/worktree while HEAD never moved — wedging every later op
            // ("Your local changes ... would be overwritten") and reading as a
            // phantom claim. The seal is atomic: restore the unmoved HEAD
            // (best-effort, like the §14 un-seal) before reporting the failure.
            let _ = run(&self.checkout, &["reset", "--hard", "HEAD"], None);
            return Err(contended());
        }
        Ok(sha)
    }

    fn unseal(&self, sha: &str) -> io::Result<()> {
        run(&self.checkout, &["reset", "--hard", sha], None)?;
        Ok(())
    }

    fn close(&self, dir: &Path) -> io::Result<()> {
        run(&self.checkout, &["worktree", "remove", "--force", &dir.to_string_lossy()], None)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
