//! §11 delivery acts — the [`crate::delivery::Repo`] trait impl for [`Project`].
//!
//! The worktree lifecycle (materialize/release/discard) and the direct
//! local-squash delivery, lifted from [`super`] so the [`Project`] git-seam
//! plumbing (the `git`/`run`/`ok` funnels and the squash helpers) stays one
//! file. Every act is idempotent — it recomputes from `(path, branch)` and
//! checks the filesystem/refs first, so a re-run is a no-op (§11).

use std::io;
use std::path::Path;

use crate::delivery::Repo;
use crate::delivery_fold::{ensure_no_merge_in_progress, ensure_no_resurrection};
use crate::delivery_repo::Project;
use crate::delivery_standing::Standing;

impl Repo for Project {
    fn materialize(&self, path: &Path, branch: &str) -> io::Result<()> {
        if path.exists() {
            return Ok(()); // create-if-absent: already materialized
        }
        // A deleted dir is the ordinary form of "absent" (crashes, tmp
        // cleaners, humans), and git may still hold its registration — a bare
        // `worktree add` then aborts as "missing but already registered"
        // (bl-b404). Prune clears exactly those stale registrations and
        // nothing else, so an unregistered absence stays a no-op.
        Self::run(&self.root, &["worktree", "prune"])?;
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

    fn work_messages(&self, branch: &str, integration: &str) -> io::Result<Vec<String>> {
        if !self.branch_exists(branch)? {
            return Ok(Vec::new()); // never worked → the caller falls back to the title
        }
        // `integration..branch` is the commits the work branch ADDED since it
        // forked; `--no-merges` drops the reintegration fold (and any author
        // hand-merge). `%B%x00` NUL-terminates each raw message so a multi-line
        // body never collides with the record boundary; the caller trims/filters.
        let range = format!("{integration}..{branch}");
        let out = Self::run(&self.root, &["log", "--no-merges", "--reverse", "--format=%B%x00", &range])?;
        Ok(out.split('\u{0}').map(str::to_string).collect())
    }

    fn push_integration(&self) -> io::Result<()> {
        // Stealth: `origin` is git's to own (bl writes bl's files, not git
        // remotes), so its absence is the structural no-op — exactly like the
        // store push with no remote (bl-2656). `get-url` exits non-zero (→ `ok`
        // false) when there is no `origin`.
        if !Self::ok(&self.root, &["remote", "get-url", "origin"])? {
            return Ok(());
        }
        let branch = self.integration()?;
        // FAIL-SOFT: close.pre already squashed the delivery onto local `main`
        // irreversibly, so a rejected push (origin moved, a history rewrite)
        // must never abort the close. Warn LOUDLY and leave local ahead — the
        // worn recovery is `git pull --rebase && git push` (no auto-sync,
        // matching bl-c3c0), not a rollback.
        if let Err(e) = Self::run(&self.root, &["push", "origin", &branch]) {
            eprintln!(
                "bl-delivery: code push to origin/{branch} pending — the delivery is on \
                 local {branch}; recover with `git -C {root} pull --rebase && git push origin \
                 {branch}` ({e})",
                root = self.root.display()
            );
        }
        Ok(())
    }

    fn is_git_repo(&self) -> io::Result<bool> {
        // An EXIT-CODE predicate, not the stdout value: `--is-inside-work-tree`
        // prints "false" for a BARE repo (the common balls deployment, where
        // delivery works fine) yet still EXITS 0 there, and exits non-zero only
        // when `root` is not a git repo at all. Reading the status (via `ok`)
        // thus accepts bare + normal worktrees and rejects only the non-repo dir
        // — and swallows the raw `fatal` so the gate can speak in balls' voice.
        Self::ok(&self.root, &["rev-parse", "--is-inside-work-tree"])
    }

    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str, marker: &str) -> io::Result<()> {
        if path.exists() {
            ensure_no_merge_in_progress(path)?;
            Self::capture(path, subject)?;
        }
        if !self.branch_exists(branch)? {
            return Ok(()); // branch never made — nothing to deliver
        }
        match self.standing(branch, integration, marker)? {
            // SETTLED (fully merged, or this incarnation's delivery survived an
            // aborted close and CONTAINS the branch — the bl-430e retry, and the
            // forge squash-merge): converge by skipping the squash.
            Standing::Settled => {
                // A delivery for this branch already stands (retry / forge
                // squash-merge / a crash between the ref-flip and the sync) —
                // the owning checkout may still carry the bl-22dd phantom; heal
                // it. Idempotent: an already-synced checkout fails the gate.
                self.reconcile(integration)?;
                return Ok(());
            }
            // A delivery stands since the fork but the branch carries content
            // beyond it — the bl-65e0 handoff onto a delivered-but-unsealed
            // close. A silent skip would strand that work; abort loudly.
            Standing::Diverged => {
                return Err(io::Error::other(format!(
                    "already delivered: a {marker} delivery commit is on {integration} since {branch} \
                     forked, but {branch} carries undelivered changes beyond it — \
                     file a new task or deliver manually"
                )))
            }
            Standing::Undelivered => {}
        }
        // Reintegration and the gate both act in the worktree; a close on a box
        // that never materialized it recreates it (create-if-absent).
        self.materialize(path, branch)?;
        Self::reintegrate(path, integration)?;
        if Self::ok(&self.root, &["diff", "--quiet", integration, branch])? {
            return Ok(()); // no tree change — empty, or reintegration dissolved the diff
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
        // `-m subject`: a plumbing `update-ref` writes a BLANK reflog message;
        // pass the delivery subject so `git reflog {integration}` is auditable
        // (carries the `[bl-id]` tag). The ref move is the BINDING commit point
        // (§14); the checkout sync that follows is its idempotent reconcile.
        Self::run(&self.root, &["update-ref", "-m", subject, &format!("refs/heads/{integration}"), &commit])?;
        self.reconcile(integration)?;
        Ok(())
    }
}
