//! §8 terminus git plumbing — the change worktree and the SEAL.
//!
//! [`Terminus`] is the narrow seam the op lifecycle ([`crate::lifecycle`])
//! drives: make the change worktree off the terminus, SEAL (commit + integrate
//! the worktree onto the terminus, atomically), un-seal a post-abort (§14
//! tier-1), and tear the worktree down. [`Git`] is the real implementation,
//! shelling out to git against one `operating/` checkout; the lifecycle is
//! unit-tested against a fake while [`Git`] is tested here on throwaway repos.
//!
//! Topology: `operating/` is a checkout with the terminus branch at commit `T`.
//! A change worktree is a DETACHED worktree created at `T`; balls + plugins
//! edit it, then SEAL commits it to `C` (parent `T`) and fast-forwards the
//! terminus onto `C` in one act. A post-seal abort `git reset --hard`s the
//! terminus back to `T` — local and reversible, because core never pushes (§14).

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The git acts the §8 seal needs, behind a seam so the lifecycle can be tested
/// without a real repo. Each method is one atomic git act on the terminus.
pub trait Terminus {
    /// The terminus tip — captured before the seal so a post-abort can un-seal.
    fn head(&self) -> io::Result<String>;
    /// (§8.1) Make the change worktree at `dir`, detached at the terminus tip.
    fn open(&self, dir: &Path) -> io::Result<()>;
    /// (§8.3) SEAL: commit everything in `dir` with `message`, then fast-forward
    /// the terminus onto it — atomically. Returns the sealed commit sha.
    fn seal(&self, dir: &Path, message: &str) -> io::Result<String>;
    /// (§14 tier-1) Un-seal a post-abort: reset the terminus back to `sha`.
    fn unseal(&self, sha: &str) -> io::Result<()>;
    /// (§8.5 / §14) Remove the change worktree — teardown, or discard on abort.
    fn close(&self, dir: &Path) -> io::Result<()>;
}

/// The real [`Terminus`]: shells out to git against one `operating/` checkout.
pub struct Git {
    operating: PathBuf,
}

impl Git {
    /// Operate against the terminus checkout rooted at `operating`.
    pub fn at(operating: &Path) -> Self {
        Self { operating: operating.to_path_buf() }
    }

    /// Run `git -C <cwd> <args>`, optionally feeding `stdin`, returning stdout.
    /// A non-zero exit becomes an [`io::Error`] carrying git's stderr — the one
    /// git-invocation site, so every method funnels its failure path here.
    fn run(cwd: &Path, args: &[&str], stdin: Option<&str>) -> io::Result<String> {
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
}

impl Terminus for Git {
    fn head(&self) -> io::Result<String> {
        Ok(Self::run(&self.operating, &["rev-parse", "HEAD"], None)?.trim().to_string())
    }

    fn open(&self, dir: &Path) -> io::Result<()> {
        Self::run(&self.operating, &["worktree", "add", "--detach", &dir.to_string_lossy(), "HEAD"], None)?;
        Ok(())
    }

    fn seal(&self, dir: &Path, message: &str) -> io::Result<String> {
        Self::run(dir, &["add", "-A"], None)?;
        Self::run(dir, &["commit", "-F", "-"], Some(message))?;
        let sha = Self::run(dir, &["rev-parse", "HEAD"], None)?.trim().to_string();
        Self::run(&self.operating, &["merge", "--ff-only", &sha], None)?;
        Ok(sha)
    }

    fn unseal(&self, sha: &str) -> io::Result<()> {
        Self::run(&self.operating, &["reset", "--hard", sha], None)?;
        Ok(())
    }

    fn close(&self, dir: &Path) -> io::Result<()> {
        Self::run(&self.operating, &["worktree", "remove", "--force", &dir.to_string_lossy()], None)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
