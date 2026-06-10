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
//! pre-commit gate before anything lands (bl-ee85). The un-squash is a derived
//! reset (no stored state) keyed on the delivery tag.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::delivery::Repo;
use crate::delivery_fold::{ensure_no_merge_in_progress, ensure_no_resurrection};
use crate::task::Task;

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

    /// Run `git -C <cwd> <args>`, returning stdout; a non-zero exit becomes an
    /// [`io::Error`] carrying git's stderr (the one failure funnel).
    pub(crate) fn run(cwd: &Path, args: &[&str]) -> io::Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
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
        Ok(Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success())
    }

    /// Does local branch `branch` exist?
    fn branch_exists(&self, branch: &str) -> io::Result<bool> {
        Self::ok(&self.root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")])
    }

    /// Is `branch` SETTLED on `integration` — carrying nothing undelivered?
    /// True when every branch commit is already on integration
    /// (`--is-ancestor`: the fresh or fully-merged branch) or a `marker`
    /// delivery commit landed on integration since the branch forked (this
    /// incarnation's squash survived — scoped to the fork so a reused id's
    /// PRIOR delivery, always an ancestor of the fork point, cannot
    /// false-positive; bl-430e/§11). The one predicate `deliver`'s retry-skip
    /// and the prime prune ([`Project::prune`]) both read through. The branch
    /// must exist.
    pub(crate) fn settled(&self, branch: &str, integration: &str, marker: &str) -> io::Result<bool> {
        if Self::ok(&self.root, &["merge-base", "--is-ancestor", branch, integration])? {
            return Ok(true);
        }
        let base = Self::run(&self.root, &["merge-base", integration, branch])?.trim().to_string();
        Ok(!self.marked(&format!("{base}..{integration}"), marker)?.is_empty())
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
    fn capture(path: &Path, subject: &str) -> io::Result<()> {
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
    fn reintegrate(path: &Path, integration: &str) -> io::Result<()> {
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
    fn gate(path: &Path) -> io::Result<()> {
        let printed = Self::run(path, &["rev-parse", "--git-path", "hooks/pre-commit"])?;
        let hook = path.join(printed.trim());
        let Ok(meta) = fs::metadata(&hook) else {
            return Ok(()); // no hook → an ungated project delivers as before
        };
        if meta.permissions().mode() & 0o111 == 0 {
            return Ok(()); // git's rule: a non-executable hook is ignored
        }
        let status = Command::new(&hook).current_dir(path).stdout(Stdio::from(io::stderr())).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("delivery gate {} failed: {status}", hook.display())))
        }
    }

    /// `delivered_in` (§11): the delivery commits carrying `marker` (`[<id>]`) on
    /// `integration`, NEWEST FIRST — the derived "where was `<id>` delivered?"
    /// query, no stored field. Recency order resolves the id-reuse ambiguity
    /// bl-d7a5 deferred: a reused id only begins after the prior incarnation
    /// CLOSED, so deliveries are monotonic with incarnations and the
    /// k-th-most-recent incarnation maps to the k-th element here — the same
    /// live-first-else-most-recent walk §9 applies to the ball file. The
    /// `--grep` is `--fixed-strings` so the `[`/`]` are matched literally, not as
    /// a regex. Empty when `<id>` was never delivered. (`git log`'s default order
    /// IS recency, so this is "do not reverse it", not extra sorting.)
    pub fn delivered_in(&self, integration: &str, marker: &str) -> io::Result<Vec<String>> {
        self.marked(integration, marker)
    }

    /// The `marker`-tagged commits reachable from `revs` (a ref or a range),
    /// newest first — the one tag-scan both [`Project::delivered_in`] and
    /// `deliver`'s retry-idempotence check read through.
    fn marked(&self, revs: &str, marker: &str) -> io::Result<Vec<String>> {
        let grep = format!("--grep={marker}");
        let out = Self::run(&self.root, &["log", "--format=%H", "--fixed-strings", &grep, revs])?;
        Ok(out.lines().map(str::to_string).collect())
    }
}

impl Repo for Project {
    fn materialize(&self, path: &Path, branch: &str) -> io::Result<()> {
        if path.exists() {
            return Ok(()); // create-if-absent: already materialized
        }
        let dst = path.to_string_lossy();
        if self.branch_exists(branch)? {
            Self::run(&self.root, &["worktree", "add", &dst, branch])?;
        } else {
            Self::run(&self.root, &["worktree", "add", &dst, "-b", branch])?;
        }
        Ok(())
    }

    fn release(&self, path: &Path) -> io::Result<()> {
        if !path.exists() {
            return Ok(()); // remove-if-present
        }
        Self::run(&self.root, &["worktree", "remove", "--force", &path.to_string_lossy()])?;
        Ok(())
    }

    fn discard(&self, path: &Path, branch: &str) -> io::Result<()> {
        self.release(path)?;
        if self.branch_exists(branch)? {
            Self::run(&self.root, &["branch", "-D", branch])?;
        }
        Ok(())
    }

    fn integration(&self) -> io::Result<String> {
        Ok(Self::run(&self.root, &["symbolic-ref", "--short", "HEAD"])?.trim().to_string())
    }

    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str, marker: &str) -> io::Result<()> {
        if path.exists() {
            ensure_no_merge_in_progress(path)?;
            Self::capture(path, subject)?;
        }
        // Nothing to deliver: branch never made, SETTLED (fully merged, or this
        // incarnation's delivery survived an aborted close — the bl-430e retry;
        // see [`Self::settled`]), or no tree change.
        if !self.branch_exists(branch)?
            || self.settled(branch, integration, marker)?
            || Self::ok(&self.root, &["diff", "--quiet", integration, branch])?
        {
            return Ok(());
        }
        // Reintegration and the gate both act in the worktree; a close on a box
        // that never materialized it recreates it (create-if-absent).
        self.materialize(path, branch)?;
        Self::reintegrate(path, integration)?;
        if Self::ok(&self.root, &["diff", "--quiet", integration, branch])? {
            return Ok(()); // reintegration dissolved the diff — already delivered
        }
        Self::gate(path)?;
        ensure_no_resurrection(&self.root, branch, integration)?;
        // After reintegration the branch tree IS the merged tree — the squash
        // is pure plumbing on it, never touching integration's checkout.
        let tree = format!("{branch}^{{tree}}");
        let tree = Self::run(&self.root, &["rev-parse", &tree])?.trim().to_string();
        let parent = Self::run(&self.root, &["rev-parse", integration])?.trim().to_string();
        let commit = Self::run(&self.root, &["commit-tree", &tree, "-p", &parent, "-m", subject])?
            .trim()
            .to_string();
        Self::run(&self.root, &["update-ref", &format!("refs/heads/{integration}"), &commit])?;
        Ok(())
    }

    fn unsquash(&self, integration: &str, marker: &str) -> io::Result<()> {
        let tip = Self::run(&self.root, &["log", "-1", "--format=%s", integration])?;
        if !tip.contains(marker) {
            return Ok(()); // tip is not our delivery commit — nothing to undo
        }
        let parent = Self::run(&self.root, &["rev-parse", &format!("{integration}^")])?.trim().to_string();
        Self::run(&self.root, &["update-ref", &format!("refs/heads/{integration}"), &parent])?;
        Ok(())
    }
}

/// The ids of every `tasks/<id>.md` in the checkout still
/// claimed by `actor` — the set `prime.post` re-materializes a worktree for
/// (§11/§12). The claimed set is not on the diffless prime wire, so the plugin
/// reads it straight off the checkout, filtering on the ball's sole occupancy
/// field ([`Task::claimant`]). Non-`.md` entries and unparseable balls are
/// skipped (a prime is best-effort and converges, not a store validator).
pub fn claimed_ids(checkout: &Path, actor: &str) -> io::Result<Vec<String>> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(checkout.join("tasks"))? {
        let path = entry?.path();
        let Some(id) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".md")) else {
            continue; // not a ball file (e.g. a stray non-`.md` entry)
        };
        let claimant = Task::parse(&fs::read_to_string(&path)?).ok().and_then(|t| t.claimant);
        if claimant.as_deref() == Some(actor) {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

/// The `tasks/<id>.md` paths the op changed in the change worktree at `cwd` —
/// how a `close.pre` hook recovers the id off the pre wire (§7). Reads the
/// working tree against `HEAD`, so a staged-or-unstaged deletion both show.
pub fn changed_task_paths(cwd: &Path) -> io::Result<Vec<String>> {
    let out = Project::run(cwd, &["diff", "--name-only", "HEAD", "--", "tasks"])?;
    Ok(out.lines().map(str::to_string).collect())
}

#[cfg(test)]
#[path = "delivery_repo_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "delivery_repo_gate_tests.rs"]
mod gate_tests;
