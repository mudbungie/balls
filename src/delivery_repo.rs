//! §11 delivery plugin — the real project-repo git seam ([`Project`]).
//!
//! [`Project`] is the production [`crate::delivery::Repo`]: it shells out to git
//! against the PROJECT repo at the invocation path, owning the `work/<id>` code
//! worktree and the direct (local-squash) delivery onto the integration branch.
//! Every act is idempotent — it recomputes from `(path, branch)` and checks the
//! filesystem/refs first, so a re-run is a no-op rather than an error (§11). The
//! squash itself is plumbing (`commit-tree` + `update-ref`) so it never disturbs
//! a checked-out integration working tree — the work happens in the code
//! worktree, where delivery folds integration in and runs the repo's own
//! pre-commit gate before anything lands (bl-ee85). The squash is the BINDING
//! commit point (§14): an abort never resets it — a retried close detects it
//! by its delivery tag and converges ([`crate::delivery_standing`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The production [`Repo`]: git against one project-repo root.
pub struct Project {
    pub(crate) root: PathBuf,
}

impl Project {
    /// Operate against the project repo rooted at `root` (the §7 invocation path).
    #[must_use]
    pub fn at(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }

    /// `git -C <cwd> <args>` as an unspawned [`Command`] — the one place the
    /// binary name and the `-C` cwd flag are spelled. Callers set only their own
    /// stdio + exit policy ([`Self::run`] captures, [`Self::ok`] discards,
    /// `standing` pipes for stdout).
    pub(crate) fn git(cwd: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(cwd).args(args);
        cmd
    }

    /// Run `git -C <cwd> <args>`, returning stdout; a non-zero exit becomes an
    /// [`io::Error`] carrying git's stderr (the one failure funnel).
    pub(crate) fn run(cwd: &Path, args: &[&str]) -> io::Result<String> {
        let out = Self::git(cwd, args).stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;
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

    /// Run `git -C <cwd> <args>` purely for its exit code — a predicate (does a
    /// ref exist? do two trees differ?). `Ok(true)` on exit 0, `Ok(false)` on
    /// any non-zero; only a spawn failure is an error.
    pub(crate) fn ok(cwd: &Path, args: &[&str]) -> io::Result<bool> {
        Ok(Self::git(cwd, args).stdout(Stdio::null()).stderr(Stdio::null()).status()?.success())
    }

    /// Does local branch `branch` exist?
    pub(crate) fn branch_exists(&self, branch: &str) -> io::Result<bool> {
        Self::ok(&self.root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")])
    }

    /// Capture any pending worktree work onto `branch` as a commit (squashed
    /// away later), so an uncommitted change is never lost at delivery.
    /// `--no-verify`: the delivery gate ([`Self::gate`]) runs ONCE, later, on
    /// the final delivered tree — not here, where it would fire only when the
    /// worktree happened to be dirty (the bl-ee85 asymmetry). The caller has
    /// already run the strict-fold guard
    /// ([`crate::delivery_fold::ensure_no_merge_in_progress`]) — over a
    /// half-merge, this `add -A` + commit would CONCLUDE the merge with a
    /// silent work-side resolution (bl-a04a).
    pub(crate) fn capture(path: &Path, subject: &str) -> io::Result<()> {
        Self::run(path, &["add", "-A"])?;
        if Self::ok(path, &["diff", "--cached", "--quiet"])? {
            return Ok(()); // nothing staged — the worktree is clean
        }
        Self::run(path, &["commit", "--no-verify", "-m", subject])?;
        Ok(())
    }

    /// Fold `integration` into the work branch IN the worktree, so the tree the
    /// gate checks IS the tree the squash delivers even when integration moved
    /// since claim. STRICT (bl-a04a): git's default merge, no `-X`/strategy
    /// side-picking ever — anything git marks conflicted (modify/delete and
    /// rename/delete included) aborts. Already-up-to-date is a commitless
    /// no-op; a conflict aborts the half-merge (the worktree stays clean for
    /// the agent to merge by hand) and surfaces as the delivery-conflict error.
    pub(crate) fn reintegrate(path: &Path, integration: &str) -> io::Result<()> {
        if let Err(e) = Self::run(path, &["merge", "--no-verify", "--no-edit", integration]) {
            let _ = Self::run(path, &["merge", "--abort"]); // best-effort: a never-started merge has nothing to abort
            return Err(io::Error::other(format!("delivery conflict merging {integration} into the work branch: {e}")));
        }
        Ok(())
    }

    /// The delivery gate (bl-ee85): run the project repo's own `pre-commit`
    /// hook — resolved exactly as git resolves it (`--git-path` honors
    /// `core.hooksPath`), skipped exactly as git skips it (absent or
    /// non-executable) — against the worktree holding the to-be-delivered tree.
    /// The squash is plumbing and would silently bypass the hook every porcelain
    /// commit runs; this restores that gate at the one moment it is
    /// representative: after capture + reintegration. A failure aborts the
    /// close BEFORE the seal, so the task stays claimed and the worktree stays
    /// up for the fix. The hook's stdout joins stderr — diagnostics, never the
    /// product channel (§6).
    pub(crate) fn gate(path: &Path) -> io::Result<()> {
        let printed = Self::run(path, &["rev-parse", "--git-path", "hooks/pre-commit"])?;
        let hook = path.join(printed.trim());
        let Ok(meta) = fs::metadata(&hook) else {
            return Ok(()); // no hook → an ungated project delivers as before
        };
        if !is_executable(&meta) {
            return Ok(()); // git's rule: a non-executable hook is ignored
        }
        let status = Command::new(&hook).current_dir(path).stdout(Stdio::from(io::stderr())).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("delivery gate {} failed: {status}", hook.display())))
        }
    }

    /// The `marker`-tagged commits reachable from `revs` (a ref or a range),
    /// NEWEST FIRST — the one tag-scan the retry standing ([`Project::standing`])
    /// reads through, and the derived "where was `<id>` delivered?" query (§11):
    /// no stored field. Recency order resolves the id-reuse ambiguity bl-d7a5
    /// deferred — a reused id only begins after the prior incarnation CLOSED, so
    /// deliveries are monotonic with incarnations and the k-th-most-recent
    /// incarnation maps to the k-th element, the same live-first-else-most-recent
    /// walk §9 applies to the ball file. The `--grep` is `--fixed-strings` so the
    /// `[`/`]` match literally, not as a regex. Empty when `marker` is absent.
    /// (`git log`'s default order IS recency, so this is "do not reverse it".)
    pub(crate) fn marked(&self, revs: &str, marker: &str) -> io::Result<Vec<String>> {
        let grep = format!("--grep={marker}");
        let out = Self::run(&self.root, &["log", "--format=%H", "--fixed-strings", &grep, revs])?;
        Ok(out.lines().map(str::to_string).collect())
    }
}

/// The `tasks/<id>.md` paths the op changed in the change worktree at `cwd` —
/// how a `close.pre` hook recovers the id off the pre wire (§7). Reads the
/// working tree against `HEAD`, so a staged-or-unstaged deletion both show.
pub fn changed_task_paths(cwd: &Path) -> io::Result<Vec<String>> {
    let out = Project::run(cwd, &["diff", "--name-only", "HEAD", "--", "tasks"])?;
    Ok(out.lines().map(str::to_string).collect())
}

/// Whether `meta` describes an executable regular file. On Unix this is the
/// owner-or-group-or-other `+x` bit — git's own rule for hook execution. On
/// Windows there is no executable bit and git-for-Windows resolves hook
/// runnability at launch time (extension / shebang), so we report every file
/// as executable here and let [`Command::new`] surface a real failure if the
/// hook can't actually be launched.
#[cfg(unix)]
fn is_executable(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(windows)]
fn is_executable(_meta: &fs::Metadata) -> bool {
    true
}

// The [`crate::delivery::Repo`] trait impl (worktree lifecycle + squash
// delivery) lives in a sibling; an `impl` block registers on [`Project`]
// regardless of module, so no re-export is needed.
#[path = "delivery_repo_acts.rs"]
mod acts;

#[cfg(test)]
#[path = "delivery_repo_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "delivery_repo_gate_tests.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "delivery_repo_push_tests.rs"]
mod push_tests;
