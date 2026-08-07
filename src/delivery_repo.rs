//! §11 delivery plugin — the real project-repo git seam ([`Project`]).
//!
//! [`Project`] is the production [`crate::delivery::Repo`]: it shells out to git
//! against the PROJECT repo at the invocation path, owning the `work/<id>` code
//! worktree and the direct (local-squash) delivery onto the integration branch.
//! Every act is idempotent — it recomputes from `(path, branch)` and checks the
//! filesystem/refs first, so a re-run is a no-op rather than an error (§11). The
//! squash itself is plumbing (`commit-tree` + `update-ref`) so it never disturbs
//! a checked-out integration working tree — the work happens in the code
//! worktree, whose tree delivery runs the repo's own pre-commit gate against
//! before anything lands (bl-ee85). That tree must ALREADY carry the target
//! (bl-a1a4): delivery validates and advances, it never reconciles. The squash
//! is the BINDING
//! commit point (§14): an abort never resets it — a retried close detects it
//! by its delivery tag and converges ([`crate::delivery_standing`]).

use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The production [`crate::delivery::Repo`]: git against one project-repo root.
#[derive(Debug)]
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
    /// delivery environment, binary name and cwd are constructed. Ambient Git
    /// controls cannot cross [`crate::safegit::delivery_at`]; repo-local config
    /// and author/committer identity still do. Callers set only their own stdio
    /// and exit policy ([`Self::run`] captures, [`Self::ok`] discards, `standing`
    /// pipes for stdout).
    pub(crate) fn git(cwd: &Path, args: &[&str]) -> Command {
        let mut cmd = crate::safegit::delivery_at(cwd);
        cmd.args(args);
        cmd
    }

    /// Run `git -C <cwd> <args>`, returning stdout; a non-zero exit becomes an
    /// [`io::Error`] carrying git's stderr (the one failure funnel).
    pub(crate) fn run(cwd: &Path, args: &[&str]) -> io::Result<String> {
        Self::feed(cwd, args, None)
    }

    /// [`Self::run`] with `stdin` piped to the child — the ARGV-FREE message
    /// channel (bl-a500). A commit message is unbounded author text: the
    /// composed delivery message concatenates every `work/<id>` commit body, and
    /// spelling it as a `-m` argument dies at the kernel's per-argument ceiling
    /// (`MAX_ARG_STRLEN`, 128 KiB on Linux) with a bare `Argument list too long
    /// (os error 7)` — observed at 142 KB on a real close, with the delivery
    /// already gated and nothing landed. Every git call that carries a MESSAGE
    /// therefore takes `-F -` and passes it here; argv carries only refs, paths
    /// and the single-line reflog label. Safe against the 64 KiB pipe buffer
    /// because the commands fed this way (`commit`, `commit-tree`) drain stdin
    /// before writing their own short output.
    pub(crate) fn feed(cwd: &Path, args: &[&str], stdin: Option<&str>) -> io::Result<String> {
        let mut cmd = Self::git(cwd, args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        let mut child = cmd.spawn()?;
        // TAKEN, not borrowed: the pipe must CLOSE at the end of this block, or
        // the child waits forever for an EOF that never comes.
        if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
            pipe.write_all(text.as_bytes())?;
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

    /// EVERY root-commit reachable from HEAD, newest-first:
    /// `git rev-list --max-parents=0 HEAD` prints one root per line, and a
    /// multi-root repo (an unrelated history merged in — vendoring) has more than
    /// one. These are the project identities this checkout answers to; the claim
    /// guard (bl-0161) admits a ball whose recorded root is ANY of them, so
    /// merging an unrelated history never flips identity and strands earlier
    /// balls. Empty when `root` is not a git repo, carries no commit yet, or any
    /// git call fails — fail-open, the guard withholds nothing it cannot prove.
    /// The set-returning read is the reusable primitive: root-aware `list`
    /// (bl-5965) scopes on the same call.
    #[must_use]
    pub fn root_commits(&self) -> Vec<String> {
        Self::run(&self.root, &["rev-list", "--max-parents=0", "HEAD"])
            .map(|out| out.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// This project's canonical, REMOTE-FREE root-commit stamp: the first
    /// (newest) of [`Self::root_commits`]. Intrinsic to history and identical
    /// across clones/hosts, it is what `create` records on a ball (bl-1ce7).
    /// `None` off a non-repo / commitless checkout — a ball created there records
    /// nothing and is unconstrained (back-compat). The stamp stays singular; the
    /// SET is the read side (the guard, and future list scope).
    #[must_use]
    pub fn root_commit(&self) -> Option<String> {
        self.root_commits().into_iter().next()
    }

    /// Capture any pending worktree work onto `branch` as a commit (squashed
    /// away later), so an uncommitted change is never lost at delivery.
    /// `--no-verify`: the delivery gate ([`Self::gate`]) runs ONCE, later, on
    /// the final delivered tree — not here, where it would fire only when the
    /// worktree happened to be dirty (the bl-ee85 asymmetry). The caller has
    /// already run the strict-fold guard
    /// ([`crate::delivery_fold::ensure_no_merge_in_progress`]) — over a
    /// half-merge, this `add -A` + commit would CONCLUDE the merge with a
    /// silent work-side resolution (bl-a04a). Capture runs BEFORE the ancestry
    /// precondition ([`crate::delivery_fold::ensure_target_incorporated`]) and
    /// survives its refusal on purpose: the remedy the refusal prescribes is
    /// `git merge <target>` in this very worktree, and git refuses that over
    /// local modifications. Committing the closer's own pending work onto their
    /// own branch is not reconciliation — it moves no target and merges nothing.
    ///
    /// `subject` is the delivery message's SUBJECT LINE, never the whole
    /// message (bl-a500). The capture commit is bookkeeping that the squash
    /// erases, but it survives on `work/<id>` across an ABORTED close — and the
    /// next attempt reads it back through [`crate::delivery::Repo::work_messages`]. Labelling it
    /// with the composed message therefore folded that message into the next
    /// composition, which the next capture folded in again: the delivery
    /// message DOUBLED per retry, reaching 142 KB after four (the reported
    /// blow-up). One line is a label; it cannot compound.
    pub(crate) fn capture(path: &Path, subject: &str) -> io::Result<()> {
        Self::run(path, &["add", "-A"])?;
        if Self::ok(path, &["diff", "--cached", "--quiet"])? {
            return Ok(()); // nothing staged — the worktree is clean
        }
        Self::feed(path, &["commit", "--no-verify", "-F", "-"], Some(subject))?;
        Ok(())
    }

    /// The delivery gate (bl-ee85): run the project repo's own `pre-commit`
    /// hook — resolved exactly as git resolves it (`--git-path` honors
    /// `core.hooksPath`), skipped exactly as git skips it (absent or
    /// non-executable) — against the worktree holding the to-be-delivered tree.
    /// The squash is plumbing and would silently bypass the hook every porcelain
    /// commit runs; this restores that gate at the one moment it is
    /// representative: after capture, on a source tree the ancestry precondition
    /// has already proved carries the target (bl-a1a4). A failure aborts the
    /// close BEFORE the seal, so the task stays claimed and the worktree stays
    /// up for the fix. The hook receives the same rebuilt environment as the
    /// Git children, so its own nested Git cannot recover caller-supplied
    /// config/redirect controls. The hook's stdout joins stderr — diagnostics,
    /// never the product channel (§6).
    pub(crate) fn gate(path: &Path) -> io::Result<()> {
        let printed = Self::run(path, &["rev-parse", "--git-path", "hooks/pre-commit"])?;
        let hook = path.join(printed.trim());
        let Ok(meta) = fs::metadata(&hook) else {
            return Ok(()); // no hook → an ungated project delivers as before
        };
        if !is_executable(&meta) {
            return Ok(()); // git's rule: a non-executable hook is ignored
        }
        let mut cmd = Command::new(&hook);
        crate::safegit::delivery_env(&mut cmd);
        let status = cmd
            .current_dir(path)
            .stdout(Stdio::from(io::stderr()))
            .status()?;
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
#[path = "delivery_repo_deliver_tests.rs"]
mod deliver_tests;

#[cfg(test)]
#[path = "delivery_repo_gate_tests.rs"]
mod gate_tests;
