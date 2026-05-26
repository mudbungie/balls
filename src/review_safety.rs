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

/// `git add -A` for the worker's worktree, followed by a defensive
/// `git rm --cached --ignore-unmatch` on every backstop path
/// (`runtime_paths::backstop_paths`).
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
    args.extend(crate::runtime_paths::backstop_paths());
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
    let backstop = crate::runtime_paths::backstop_paths();
    Ok(out
        .lines()
        .filter(|p| {
            backstop
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
    // Mirror the squash mechanism (post-bl-cb73): always rewind the
    // branch ref with `update-ref`, then — only when the integration
    // branch is checked out at a non-bare `store.root` — re-sync the
    // work tree to the rewound tip. Same predicate the squash path
    // uses for its post-update `reset --hard`, so rewind and squash
    // always touch the work tree (or don't) in lockstep.
    let refname = format!("refs/heads/{main_branch}");
    git::git_update_ref(&store.root, &refname, pre_main_sha)?;
    if crate::bare_squash::integration_branch_is_checked_out(&store.root, main_branch)? {
        git::git_reset_hard(&store.root, "HEAD")?;
    }
    Ok(())
}

/// Build the error returned when a squash brought runtime paths in.
/// Same wording in tests and at the review site so the caller can
/// match on it without duplicating the message.
pub fn runtime_in_squash_error(id: &str, paths: &[String]) -> BallError {
    // The example subpaths derive from the same `backstop_paths()`
    // table the rejection itself uses (bl-228d/bl-0151), so a new
    // sidecar can never leave this prose stale. Every backstop path is
    // a `.balls/` subpath, with that prefix factored into the sentence.
    let names: Vec<String> = crate::runtime_paths::backstop_paths()
        .iter()
        .map(|&p| format!("`{}`", p.strip_prefix(".balls/").unwrap_or(p)))
        .collect();
    let last = names.len() - 1;
    let subpaths = format!("{}, or {}", names[..last].join(", "), names[last]);
    BallError::Other(format!(
        "bl review {id} aborted: squash would deliver balls runtime path{plural} {list}. \
         The work branch has tracked a `.balls/` subpath ({subpaths}) — these are internal \
         state, not deliverables. Run `bl init` in the repo to refresh `.gitignore`, then \
         `git rm --cached` the offending paths on the work branch and retry.",
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
    let delivered_sha =
        crate::bare_squash::squash_into_main(store, branch, squash_msg, main_branch)?;
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
    // Per-task delivery (bl-d4b0) lands the squash on the task's own
    // `target_branch`, not the repo-level integration branch. Record
    // that effective branch in the review subject so the post-delivery
    // machinery (`bl sync`'s push, half-push's tag scan) finds the
    // delivery on the right branch (bl-f788). Omitted when it equals
    // the repo-level default, so untouched repos keep byte-identical
    // state subjects and old clients see no marker. The
    // `target_branch.is_some()` gate keeps the no-override path free of
    // the extra config/HEAD resolution.
    let target_marker = if had_delivery && t.target_branch.is_some() {
        let repo_default = store.load_config()?.integration_branch(&store.root)?;
        (main_branch != repo_default).then(|| main_branch.to_string())
    } else {
        None
    };
    t.status = Status::Review;
    if had_delivery {
        // bl-7523: tag the delivery's source repo so a reader on the
        // tracker (or a sibling client) can still resolve the sha
        // back to a repo it can fetch from. Only set when there was
        // an actual squash — a no-code checkpoint review has nothing
        // to attach provenance to.
        t.delivered_repo = Some(crate::repo_url::current(&store.root));
    }
    t.delivered_in = delivered_sha;
    t.touch();
    t.save(&task_path)?;
    if let Some(msg) = message {
        task_io::append_note_to(&task_path, identity, msg)?;
    }
    let state_msg = match (had_delivery, &target_marker) {
        (false, _) => format!("state: review {id} no-code"),
        (true, Some(b)) => format!("state: review {id} target={b}"),
        (true, None) => format!("state: review {id}"),
    };
    store.commit_task(id, &state_msg)?;
    Ok(())
}

#[cfg(test)]
#[path = "review_safety_tests.rs"]
mod tests;
