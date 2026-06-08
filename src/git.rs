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
use std::process::{Command, Stdio};

/// The git acts the §8 seal needs, behind a seam so the lifecycle can be tested
/// without a real repo. Each method is one atomic git act on the anvil.
pub trait Anvil {
    /// The anvil tip — captured before the seal so a post-abort can un-seal.
    fn head(&self) -> io::Result<String>;
    /// (§8.1) Make the change worktree at `dir`, detached at the anvil tip.
    fn open(&self, dir: &Path) -> io::Result<()>;
    /// (§8.3) SEAL: commit everything in `dir` with `message`, then fast-forward
    /// the anvil onto it — atomically. Returns the sealed commit sha.
    fn seal(&self, dir: &Path, message: &str) -> io::Result<String>;
    /// (§14 tier-1) Un-seal a post-abort: reset the anvil back to `sha`.
    fn unseal(&self, sha: &str) -> io::Result<()>;
    /// (§8.5 / §14) Remove the change worktree — teardown, or discard on abort.
    fn close(&self, dir: &Path) -> io::Result<()>;
}

/// The real [`Anvil`]: shells out to git against one checkout.
pub struct Git {
    checkout: PathBuf,
}

impl Git {
    /// Operate against the anvil checkout rooted at `checkout`.
    pub fn at(checkout: &Path) -> Self {
        Self { checkout: checkout.to_path_buf() }
    }

    /// Run `git -C <cwd> <args>` against the anvil — every method funnels its
    /// failure path through the shared [`run`] plumbing.
    fn run(cwd: &Path, args: &[&str], stdin: Option<&str>) -> io::Result<String> {
        run(cwd, args, stdin)
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
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(cwd).args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
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

impl Anvil for Git {
    fn head(&self) -> io::Result<String> {
        Ok(Self::run(&self.checkout, &["rev-parse", "HEAD"], None)?.trim().to_string())
    }

    fn open(&self, dir: &Path) -> io::Result<()> {
        Self::run(&self.checkout, &["worktree", "add", "--detach", &dir.to_string_lossy(), "HEAD"], None)?;
        Ok(())
    }

    fn seal(&self, dir: &Path, message: &str) -> io::Result<String> {
        Self::run(dir, &["add", "-A"], None)?;
        Self::run(dir, &["commit", "-F", "-"], Some(message))?;
        let sha = Self::run(dir, &["rev-parse", "HEAD"], None)?.trim().to_string();
        Self::run(&self.checkout, &["merge", "--ff-only", &sha], None)?;
        Ok(sha)
    }

    fn unseal(&self, sha: &str) -> io::Result<()> {
        Self::run(&self.checkout, &["reset", "--hard", sha], None)?;
        Ok(())
    }

    fn close(&self, dir: &Path) -> io::Result<()> {
        Self::run(&self.checkout, &["worktree", "remove", "--force", &dir.to_string_lossy()], None)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
