//! §11.1 — the ATTEMPT: a delivery source that is not a ball (bl-4eac).
//!
//! balls already owned the whole recursive source-to-target mechanism; what it
//! lacked was a way to reach it without manufacturing a ball, and a return value
//! (docs/design/bl-4eac-attempt-capability.md). An attempt is that reach: a
//! private source ref + index + worktree, minted at an exact target commit,
//! delivered by the SAME law `bl close` delivers a ball by
//! ([`crate::delivery_message::deliver_to`]) and returning the identities the
//! delivery already computed ([`Delivered`]).
//!
//! It is policy-blind by construction. balls owns refs, worktrees, delivery and
//! safe cleanup; it holds no notion of candidate, winner, cohort or outcome —
//! acceptance is the target's own history (the `[handle]`-tagged squash it
//! carries), cohort is `(target, base)`, and rejection is the ABSENCE of a
//! delivery. How many attempts exist, how they compare, and when a rejected one
//! expires are the caller's, spent through [`Attempt::release`] (the worktree
//! goes, the source stays addressable) and [`Attempt::discard`] (both go).
//!
//! There is no `bl` verb here and must not be one: a verb would be a second
//! entry point to a capability whose whole point is that the N = 1 ball path and
//! the N > 1 alternative paths are ONE mechanism. `bl close` is the ball
//! attempt; a linking host reaches the same delivery through this module.

use std::io;
use std::path::{Path, PathBuf};

use crate::delivery::{Delivered, Repo};
use crate::delivery_message::subject_line;
use crate::delivery_path::{attempt_branch, attempt_path, ensure_safe_invocation_path, marker, subject};
use crate::delivery_precondition::precondition_unmet;
use crate::delivery_repo::Project;
use crate::id::IdScheme;
use crate::layout::Xdg;

/// How an attempt HANDLE is minted: `at-` + 8 hex, re-rolled off the live
/// `attempt/*` set by the ordinary [`IdScheme`]. Wider than a ball id because a
/// handle is not a name anyone types — it is opaque to its holder, who binds an
/// agent to it and stores it in their own history. The `at-` tag is what keeps a
/// handle unmistakable for a ball id everywhere both appear (a delivery subject,
/// a `git log --grep`).
fn handles() -> IdScheme {
    IdScheme { prefix: "at-".to_string(), length: 8, alphabet: "0123456789abcdef".to_string() }
}

/// An OPAQUE, balls-resolved delivery target, pinned by the delivery that
/// consumes it. The field is private on purpose: a caller cannot CONSTRUCT one,
/// only ask balls for one — which is what makes "callers never construct
/// worktree paths or ref names" mechanical rather than a documented request.
///
/// Three authorities answer, and they are the three cases the recursive graph
/// has: the project repo itself ([`Project::target`] with `None` — the
/// integration branch, never hardcoded to `main`), the ball graph
/// ([`Project::target`] with an id — `work/<id>`, exactly what `bl close`
/// derives for a close-gating child, bl-7b71), an explicit validated branch
/// ([`Project::target_ref`]), and a parent attempt ([`Attempt::target`] — a
/// write-capable child targets its parent's source ref, the fractal law one
/// depth down).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target(String);

impl Project {
    /// The target a BALL delivers into: `work/<id>` when `ball` names the parent
    /// it close-gates (lazily minted at the integration head), else the
    /// integration branch — one call, the same derivation `bl close` makes, so
    /// fanning a ball obligation cannot drift from closing it.
    pub fn target(&self, ball: Option<&str>) -> io::Result<Target> {
        self.require_repo()?;
        Ok(Target(crate::delivery::target_branch(self, ball)?))
    }

    /// An EXPLICIT delivery target, validated on the way in: `branch` must exist
    /// as a local branch of this project repo. This is the "explicit repo + ref"
    /// start (a bare project repo included) — and the entry-side half of the
    /// deleted/moved-target answer, the exit-side half being the ancestry
    /// precondition and the CAS, which catch a target that moves AFTER this.
    pub fn target_ref(&self, branch: &str) -> io::Result<Target> {
        self.require_repo()?;
        if self.branch_exists(branch)? {
            return Ok(Target(branch.to_string()));
        }
        Err(io::Error::other(format!(
            "no such delivery target: {branch} is not a branch of this project repo"
        )))
    }

    /// Refuse a non-repo invocation path in balls' voice rather than letting the
    /// first git act surface a raw `fatal: not a git repository` (bl-4a88). Every
    /// attempt passes through a [`Target`], so guarding the two constructors
    /// guards the whole capability.
    fn require_repo(&self) -> io::Result<()> {
        if self.is_git_repo()? {
            return Ok(());
        }
        Err(io::Error::other(precondition_unmet(&self.root.to_string_lossy())))
    }
}

/// One write-capable delivery attempt: a private source ref, index and worktree
/// forked from an exact target commit.
///
/// **The lease is git's own.** balls adds no lockfile and no liveness probe: a
/// handle is minted fresh and re-rolled off the live set so two attempts never
/// name one ref; `git worktree add` refuses a ref already checked out elsewhere
/// so two worktrees never share one index; and the worktree path is a pure
/// function of the handle, so an attempt has exactly one place to be. What
/// remains — one CALLER handing one handle to two agents — is not detectable
/// without a liveness probe, and bl-1e98 already refused to invent one. balls
/// never returns a handle twice; who holds a returned handle is the caller's
/// fact, at the same altitude as the claim lease.
#[derive(Debug)]
pub struct Attempt {
    project: Project,
    handle: String,
    worktree: PathBuf,
    target: String,
    base: String,
}

impl Attempt {
    /// Materialize a FRESH attempt against `target`: mint a handle, fork
    /// `attempt/<handle>` at the target tip, and check it out into this
    /// attempt's own worktree. `root` is the project repo (the §7 invocation
    /// path); `xdg` places the worktree.
    pub fn open(root: &Path, xdg: &Xdg, target: &Target) -> io::Result<Attempt> {
        let project = Project::at(root);
        let handle = handles().mint(&live_handles(&project)?)?;
        bind(project, xdg, target, handle)
    }

    /// Re-materialize an EXISTING attempt by handle — the crash retry. Whatever
    /// survived (the source ref always; the worktree unless the crash or a
    /// cleaner took it) is reused, and what is missing is remade; an unknown
    /// handle is REFUSED rather than quietly minted, so a typo cannot become a
    /// new attempt.
    pub fn resume(root: &Path, xdg: &Xdg, target: &Target, handle: &str) -> io::Result<Attempt> {
        let project = Project::at(root);
        project.require_repo()?;
        if !project.branch_exists(&attempt_branch(handle))? {
            return Err(io::Error::other(format!(
                "unknown attempt handle: {handle} names no source ref in this project repo"
            )));
        }
        bind(project, xdg, target, handle.to_string())
    }

    /// This attempt's opaque handle — the only name its holder ever learns.
    #[must_use]
    pub fn handle(&self) -> &str {
        &self.handle
    }

    /// The private worktree an agent works in. Write-capable and this attempt's
    /// alone.
    #[must_use]
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// The exact commit this attempt started from: `merge-base(target, source)`,
    /// derived, never stored. For a fresh attempt that IS the tip the source ref
    /// was minted at; for a resumed one it recovers the true fork point rather
    /// than re-pinning to a target that has since moved — one formula, both
    /// cases, no special arm.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The source tip right now. With [`Attempt::base`] this is the whole of the
    /// project diff (`base..tip`, a plain git read) WITHOUT the caller ever
    /// spelling a ref name.
    pub fn tip(&self) -> io::Result<String> {
        Ok(Project::run(&self.project.root, &["rev-parse", &attempt_branch(&self.handle)])?.trim().to_string())
    }

    /// This attempt AS a target — what a write-capable child attempt delivers
    /// into. The one legitimate use of an attempt's ref name, expressible
    /// without exposing it; the recursion needs nothing else.
    #[must_use]
    pub fn target(&self) -> Target {
        Target(attempt_branch(&self.handle))
    }

    /// Deliver this attempt onto its target under the one delivery law: the
    /// target must ALREADY be incorporated (bl-a1a4 — a stale source refuses
    /// before anything merges, gates, squashes or moves, and balls never
    /// reconciles it), then the repo's own `pre-commit` gate on the exact source
    /// tree, the `[<handle>]`-tagged squash, and the CAS advance.
    ///
    /// `summary` becomes the delivery subject tagged with the handle (its first
    /// line only — a subject is one line by construction); `note` is optional
    /// body narration, joined ahead of the attempt's own commit messages exactly
    /// as a close's `-m` is (§5: no subject override, anywhere).
    pub fn deliver(&self, summary: &str, note: Option<&str>) -> io::Result<Delivered> {
        crate::delivery_message::deliver_to(
            &self.project,
            &self.worktree,
            &attempt_branch(&self.handle),
            &self.target,
            &subject(subject_line(summary), &self.handle),
            note,
            &marker(&self.handle),
        )
    }

    /// Release the WORKTREE and keep the source ref — retention separated from
    /// cleanup. A rejected attempt changes no target and stays fully
    /// addressable: `base..tip` still reads, the ref still enumerates. When its
    /// retention expires is the caller's call, spent as [`Attempt::discard`].
    pub fn release(&self) -> io::Result<()> {
        self.project.release(&self.worktree)
    }

    /// Explicit cleanup: release the worktree AND delete the source ref. The
    /// attempt is gone; a [`Attempt::resume`] of its handle is refused
    /// afterwards. balls performs this only when asked — it never sweeps
    /// attempts, which is "the caller owns retention" said mechanically.
    pub fn discard(&self) -> io::Result<()> {
        self.project.discard(&self.worktree, &attempt_branch(&self.handle))
    }
}

/// The one materialization body, shared by [`Attempt::open`] and
/// [`Attempt::resume`]: ensure the source ref (create-if-absent at the target
/// tip), ensure the worktree (create-if-absent, healing a stale registration a
/// crash left — bl-b404), and derive the base. Both entry points are the same
/// act over a handle that is either fresh or given, so a resume is the general
/// path with the mint already done, not a recovery mode.
fn bind(project: Project, xdg: &Xdg, target: &Target, handle: String) -> io::Result<Attempt> {
    let invocation = project.root.to_string_lossy().into_owned();
    ensure_safe_invocation_path(&invocation)?;
    let branch = attempt_branch(&handle);
    project.mint(&branch, &target.0)?;
    let worktree = attempt_path(xdg, &invocation, &handle);
    project.materialize(&worktree, &branch)?;
    let base = Project::run(&project.root, &["merge-base", &target.0, &branch])?.trim().to_string();
    Ok(Attempt { project, handle, worktree, target: target.0.clone(), base })
}

/// Every live attempt handle in this project repo — the `taken` set the handle
/// mint re-rolls off. Derived from the refs themselves (§0 derive-don't-store):
/// `attempt/*` IS the registry, so there is nothing to keep in sync.
fn live_handles(project: &Project) -> io::Result<Vec<String>> {
    let refs = Project::run(&project.root, &["for-each-ref", "--format=%(refname:short)", "refs/heads/attempt/"])?;
    Ok(refs.lines().filter_map(|b| b.strip_prefix("attempt/")).map(str::to_string).collect())
}

#[cfg(test)]
#[path = "attempt_tests.rs"]
mod tests;
