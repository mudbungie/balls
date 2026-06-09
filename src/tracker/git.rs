//! The tracker's git runner — the one remote-talking primitive (§0/§13).
//!
//! The tracker is balls' only component that touches a remote, and every
//! tracker act is an ordinary `git fetch` / `git merge` / `git push` on the
//! state branch (no database, no daemon). [`git`] is the single spawn site, so
//! every handler funnels its failure through one place that carries git's
//! stderr — a non-ff merge or a rejected push surfaces verbatim, which is the
//! contention signal the §13 ff-only contract relies on.
//!
//! This is deliberately separate from [`crate::git`] (the §8 anvil seal):
//! that seam never talks to a remote, this one only does. Keeping them apart is
//! the §0 split — core is local-only, remote-talk is the plugin's alone.

use std::io;
use std::path::Path;
use std::process::Stdio;

/// Run `git -C <cwd> <args>`, returning trimmed stdout. A non-zero exit becomes
/// an [`io::Error`] carrying git's stderr — the contention/abort signal. The
/// command is built through [`crate::safegit::at`], so the redirection `GIT_*`
/// env is stripped and the `ext::` transport denied (bl-2d6d).
pub fn git(cwd: &Path, args: &[&str]) -> io::Result<String> {
    let out = crate::safegit::at(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(io::Error::other(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn returns_trimmed_stdout_on_success() {
        let tmp = TempDir::new().unwrap();
        git(tmp.path(), &["init", "-q"]).unwrap();
        let bare = git(tmp.path(), &["rev-parse", "--is-bare-repository"]).unwrap();
        assert_eq!(bare, "false"); // trimmed: no trailing newline
    }

    #[test]
    fn a_nonzero_exit_carries_gits_stderr() {
        let tmp = TempDir::new().unwrap();
        let err = git(tmp.path(), &["rev-parse", "HEAD"]).unwrap_err();
        assert!(err.to_string().contains("git rev-parse HEAD"));
    }
}
