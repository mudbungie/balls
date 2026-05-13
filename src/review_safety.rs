//! Safety rails around `bl review`'s integration-branch mutation.
//!
//! Two failure modes prompted this module (bl-0dc3):
//!
//! 1. The worker's `.balls/{local,tasks,worktree}` runtime symlinks
//!    must never reach the integration branch. They are repo-internal
//!    state; landing them as user changes corrupts the consuming
//!    branch and bricks `bl` until the commit is reset.
//!
//! 2. If `bl review` fails *after* the squash commit lands on the
//!    integration branch, the caller is told "review failed" while
//!    main has already moved. `rewind_main` lets the review path
//!    restore the pre-review tip so a failed review is atomic from
//!    the user's perspective.
//!
//! Both rails are defense-in-depth on top of `.gitignore`: even if a
//! worktree's `.gitignore` predates `bl init` and lacks the runtime
//! entries, `add_user_changes` will not stage them and
//! `commit_touches_runtime` will reject any squash that brought them
//! in anyway (e.g. because a prior commit on the work branch tracked
//! them).

use crate::error::{BallError, Result};
use crate::git;
use crate::store::Store;
use crate::task::{Status, Task};
use crate::task_io;
use std::path::Path;

/// Runtime paths inside `.balls/` that are bl's internal state and
/// must never travel through a squash to the integration branch.
/// Kept in sync with `store_init::ensure_main_gitignore`.
pub(crate) const RUNTIME_PATHS: &[&str] = &[".balls/local", ".balls/tasks", ".balls/worktree"];

/// `git add -A` for the worker's worktree, followed by a defensive
/// `git rm --cached --ignore-unmatch` on the three runtime paths.
/// Pathspec-based exclusion (`:(exclude).balls/...`) is rejected by
/// git's `add` when the paths are gitignored, so we stage first then
/// unstage the runtime paths. This works whether the runtime paths
/// were gitignored (normal: no-op unstage), staged as new untracked
/// files (stale .gitignore: removed from index), or carried into the
/// index from prior tracked commits (force-unstaged as a deletion on
/// `work/<id>`). The working-tree symlinks stay in place because
/// `--cached` only touches the index.
pub fn add_user_changes(wt: &Path) -> Result<()> {
    git::run_git_ok(wt, &["add", "-A"])?;
    let mut args: Vec<&str> = vec!["rm", "--cached", "--ignore-unmatch", "-r", "--"];
    args.extend(RUNTIME_PATHS.iter().copied());
    git::run_git_ok(wt, &args)?;
    Ok(())
}

/// Names of runtime paths touched by `sha`. Used as a backstop after
/// the squash lands: if the commit contains any of the runtime paths,
/// the review must be unwound before returning success. The check
/// works on the commit (rather than the index) so it applies even
/// when the bare-repo path lands the commit in a detached worktree.
pub fn commit_touches_runtime(repo: &Path, sha: &str) -> Result<Vec<String>> {
    let out = git::run_git_ok(repo, &["show", "--name-only", "--format=", sha])?;
    Ok(out
        .lines()
        .filter(|p| {
            RUNTIME_PATHS
                .iter()
                .any(|r| *p == *r || p.starts_with(&format!("{r}/")))
        })
        .map(str::to_string)
        .collect())
}

/// Move the integration branch back to `pre_main_sha`, undoing a
/// squash that should not have been left in place. Uses
/// `git reset --hard` on a working-tree root and `update-ref` on a
/// bare gitdir so the same call site works in both layouts. Errors
/// are intentionally returned but the review path treats them as
/// best-effort: the original review failure is what the caller cares
/// about.
pub fn rewind_main(store: &Store, main_branch: &str, pre_main_sha: &str) -> Result<()> {
    if is_bare(&store.root)? {
        let refname = format!("refs/heads/{main_branch}");
        git::run_git_ok(&store.root, &["update-ref", &refname, pre_main_sha])?;
    } else {
        git::git_reset_hard(&store.root, pre_main_sha)?;
    }
    Ok(())
}

fn is_bare(dir: &Path) -> Result<bool> {
    Ok(git::run_git_ok(dir, &["rev-parse", "--is-bare-repository"])?.trim() == "true")
}

/// Build the error returned when a squash brought runtime paths in.
/// Same wording in tests and at the review site so the caller can
/// match on it without duplicating the message.
pub fn runtime_in_squash_error(id: &str, paths: &[String]) -> BallError {
    BallError::Other(format!(
        "bl review {id} aborted: squash would deliver balls runtime path{plural} {list}. \
         The work branch has tracked `.balls/local`, `.balls/tasks`, or `.balls/worktree` — \
         these are internal state, not deliverables. Run `bl init` in the repo to refresh \
         `.gitignore`, then `git rm --cached` the offending paths on the work branch and retry.",
        plural = if paths.len() == 1 { "" } else { "s" },
        list = paths.join(", "),
    ))
}

/// Squash the work branch into the integration branch, reject the
/// commit if it carries runtime paths, then flip the task to
/// `Status::Review` on the state branch with a single commit. The
/// caller (`review::review_worktree`) wraps this in a rewind handler
/// so any failure leaves the integration branch at its pre-review
/// tip — `bl review`'s atomicity guarantee.
#[allow(clippy::too_many_arguments)]
pub fn commit_squash_and_flip(
    store: &Store,
    id: &str,
    branch: &str,
    squash_msg: &str,
    message: Option<&str>,
    identity: &str,
    pre_main_sha: &str,
    main_branch: &str,
) -> Result<()> {
    let delivered_sha = crate::bare_squash::squash_into_main(store, branch, squash_msg)?;
    if let Some(sha) = delivered_sha.as_deref() {
        let dirty = commit_touches_runtime(&store.root, sha)?;
        if !dirty.is_empty() {
            let _ = rewind_main(store, main_branch, pre_main_sha);
            return Err(runtime_in_squash_error(id, &dirty));
        }
    } else {
        eprintln!("no code delivered — checkpoint review for {id}");
    }
    let had_delivery = delivered_sha.is_some();
    let task_path = store.task_path(id)?;
    let mut t = Task::load(&task_path)?;
    t.status = Status::Review;
    t.delivered_in = delivered_sha;
    t.touch();
    t.save(&task_path)?;
    if let Some(msg) = message {
        task_io::append_note_to(&task_path, identity, msg)?;
    }
    let state_msg = if had_delivery {
        format!("state: review {id}")
    } else {
        format!("state: review {id} no-code")
    };
    store.commit_task(id, &state_msg)?;
    Ok(())
}

#[cfg(test)]
#[path = "review_safety_tests.rs"]
mod tests;
