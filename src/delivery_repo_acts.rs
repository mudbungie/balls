//! §11 delivery acts — the [`crate::delivery::Repo`] trait impl for [`Project`].
//!
//! The worktree lifecycle (materialize/release/discard) and the direct
//! local-squash delivery, lifted from [`super`] so the [`Project`] git-seam
//! plumbing (the `git`/`run`/`ok` funnels and the squash helpers) stays one
//! file. Every act is idempotent — it recomputes from `(path, branch)` and
//! checks the filesystem/refs first, so a re-run is a no-op (§11).

use std::io;
use std::path::Path;

use crate::delivery::{Delivered, Repo};
use crate::delivery_fold::{ensure_no_merge_in_progress, ensure_no_resurrection, ensure_target_incorporated};
use crate::delivery_message::subject_line;
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

    fn mint(&self, branch: &str, base: &str) -> io::Result<()> {
        if self.branch_exists(branch)? {
            return Ok(()); // create-if-absent: the ref already names a point in history
        }
        Self::run(&self.root, &["branch", branch, base])?;
        Ok(())
    }

    fn work_messages(&self, branch: &str, integration: &str) -> io::Result<Vec<String>> {
        if !self.branch_exists(branch)? {
            return Ok(Vec::new()); // never worked → the caller falls back to the title
        }
        // `integration..branch` is the commits the work branch ADDED since it
        // forked; `--no-merges` drops the closer's own reconciling merges.
        // `%B%x00` NUL-terminates each raw message so a multi-line
        // body never collides with the record boundary; the caller trims/filters.
        let range = format!("{integration}..{branch}");
        let out = Self::run(&self.root, &["log", "--no-merges", "--reverse", "--format=%B%x00", &range])?;
        Ok(out.split('\u{0}').map(str::to_string).collect())
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

    fn deliver(&self, path: &Path, branch: &str, integration: &str, message: &str, marker: &str) -> io::Result<Delivered> {
        // `message` is unbounded author text; `label` is its one-line handle —
        // the only form that may ride argv or a reflog (bl-a500).
        let label = subject_line(message);
        // Capture first, and BEFORE the ancestry precondition below — not an
        // exception to it but its precondition in turn. Standing is read off
        // refs, so uncommitted work must be on the branch before it is
        // classified or a dirty worktree over an unmoved target reads Settled
        // and its work is never delivered. And the remedy the precondition
        // prescribes is `git merge <target>` in this very worktree, which git
        // refuses over local modifications. Committing the closer's own pending
        // work onto the closer's own branch merges nothing and moves no target
        // ref, so it is outside what the precondition forbids ahead of itself.
        if path.exists() {
            ensure_no_merge_in_progress(path)?;
            Self::capture(path, label)?;
        }
        // Pin the target tip ONCE (bl-8b89) — see the long note at the squash
        // below for why one read serves as precondition comparison point, squash
        // parent, no-resurrection base and CAS old-value. It is read HERE, ahead
        // of the no-op arms, only so those arms can report the same `base` a
        // landing one would (bl-4eac): the pin is a `rev-parse`, it moves
        // nothing, and no arm between here and the precondition re-reads it.
        let base = Self::run(&self.root, &["rev-parse", integration])?.trim().to_string();
        let identities = |source, commit| Delivered {
            target: integration.to_string(),
            base: base.clone(),
            source,
            commit,
        };
        if !self.branch_exists(branch)? {
            return Ok(identities(None, None)); // branch never made — nothing to deliver
        }
        let source = Self::run(&self.root, &["rev-parse", branch])?.trim().to_string();
        match self.standing(branch, integration, marker)? {
            // SETTLED (fully merged, or this incarnation's delivery survived an
            // aborted close and CONTAINS the branch — the bl-430e retry, and the
            // forge squash-merge): converge by skipping the squash.
            Standing::Settled(standing) => {
                // A delivery for this branch already stands (retry / forge
                // squash-merge / a crash between the ref-flip and the sync) —
                // the owning checkout may still carry the bl-22dd phantom; heal
                // it. Idempotent: an already-synced checkout fails the gate.
                self.reconcile(integration)?;
                // The converged retry reports the STANDING commit as its own:
                // that is where this source's content actually is (bl-4eac).
                return Ok(identities(Some(source), standing));
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
        // The pin above (bl-8b89) is this single value: the ancestry
        // precondition's comparison point, the squash parent, the CAS
        // old-value, and the no-resurrection comparison point. Re-reading the
        // ref after the gate validated only "integration has not moved since a
        // moment ago" — letting a mid-gate sibling landing either false-fire
        // the resurrection invariant (its paths look like excess) or, when its
        // paths were a subset of this branch's authored set, pass every guard
        // and be SILENTLY REVERTED by a squash computed from the pre-move tree.
        // It is the ONLY read of `integration` the delivery makes (bl-9522), so
        // there is no second read to disagree with the pin; the branch NAME
        // survives in the voice alone.
        //
        // THE PRECONDITION (bl-a1a4). Delivery is a validation and atomic-advance
        // boundary, never a merge queue: the source must ALREADY carry the
        // pinned target. Refused here, nothing has merged, gated, squashed or
        // moved a ref — the closer reconciles in their own worktree, tests
        // there, and retries. This is where the automatic fold used to be.
        ensure_target_incorporated(&self.root, branch, integration, &base)?;
        // The gate acts in the worktree; a close on a box that never
        // materialized it recreates it (create-if-absent).
        self.materialize(path, branch)?;
        if Self::ok(&self.root, &["diff", "--quiet", &base, branch])? {
            // NO INCOMING DIFF: incorporating the source into the target would
            // be a no-op — an empty deliverable, or a source whose whole content
            // reached the target another way. `T..S` is the work product, and
            // here it is empty.
            return Ok(identities(Some(source), None));
        }
        Self::gate(path)?;
        ensure_no_resurrection(&self.root, branch, &base)?;
        // The branch tree already contains the target, so the squash is pure
        // plumbing on it, never touching integration's checkout.
        let tree = format!("{branch}^{{tree}}");
        let tree = Self::run(&self.root, &["rev-parse", &tree])?.trim().to_string();
        // `-F -`: the message goes down STDIN. As a `-m` argument it died at
        // `MAX_ARG_STRLEN` — post-gate, pre-landing (bl-a500).
        let commit = Self::feed(&self.root, &["commit-tree", &tree, "-p", &base, "-F", "-"], Some(message))?
            .trim()
            .to_string();
        commit_swap(&self.root, integration, label, &commit, &base)?;
        self.reconcile(integration)?;
        Ok(identities(Some(source), Some(commit)))
    }
}

/// COMPARE-AND-SWAP the integration ref onto the fresh squash (bl-a3bb).
///
/// `parent` is the PINNED target tip (bl-8b89) — the `integration` tip read
/// once, before the ancestry precondition, i.e. the tip the gated tree carries — and
/// `commit` was minted onto it; passing it as `update-ref`'s optional old-value
/// makes the ref move CONDITIONAL — git writes only while `integration` still
/// points there. Two closes sharing one project checkout race the whole window
/// from that read through the gate to this move: without the old-value
/// `update-ref` writes UNCONDITIONALLY, so the loser overwrites the winner's
/// already-landed squash off `integration` (reflog-only recovery, unreported) —
/// and with a post-gate re-read as the old-value, a mid-gate landing whose paths
/// sat inside this branch's authored set passed the CAS and was silently
/// reverted by the pre-move squash tree. A rejected CAS is a LOUD pre-seal abort
/// — nothing overwritten, the task stays claimed and the worktree stays up; the
/// retried close then meets the ancestry precondition (bl-a1a4) and REFUSES
/// until the closer incorporates the new tip and tests it, exactly as it would
/// had the sibling landed before the delivery began. Convergence on retry is
/// preserved (§14); what is not preserved is delivery re-folding on the
/// closer's behalf. `parent` is always a real commit here (the `rev-parse integration`
/// pin errored otherwise), so the empty old-value / first-commit form never
/// arises.
///
/// `-m subject`: a plumbing `update-ref` writes a BLANK reflog message; pass the
/// delivery subject so `git reflog {integration}` is auditable (carries the
/// `[bl-id]` tag). The SUBJECT LINE, not the message — a reflog entry is one
/// line by construction, and it keeps balls' last argv-borne text bounded
/// (bl-a500). The ref move is the delivery's BINDING commit point (§14); the
/// checkout sync that follows it is the idempotent reconcile.
pub(crate) fn commit_swap(root: &Path, integration: &str, subject: &str, commit: &str, parent: &str) -> io::Result<()> {
    let refname = format!("refs/heads/{integration}");
    if Project::ok(root, &["update-ref", "-m", subject, &refname, commit, parent])? {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "{integration} moved under the delivery — a concurrent close landed between the squash \
         and the ref move; nothing was overwritten. Merge the new {integration} tip into your \
         work worktree, resolve and test there, then re-run `bl close`"
    )))
}
