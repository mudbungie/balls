use crate::error::{BallError, Result};
use crate::store::{task_lock, Store};
use crate::task::{Status, Task};
use crate::{git, task, task_io};
use std::{fs, path::PathBuf};

fn with_task_lock<T>(store: &Store, id: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let _guard = task_lock(store, id)?;
    f()
}

fn claim_file_path(store: &Store, id: &str) -> PathBuf {
    store.claims_dir().join(id)
}

fn write_claim_file(store: &Store, id: &str, worker: &str) -> Result<()> {
    fs::create_dir_all(store.claims_dir())?;
    let content = format!(
        "worker={}\npid={}\nclaimed_at={}\n",
        worker, std::process::id(), chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    fs::write(claim_file_path(store, id), content)?;
    Ok(())
}

fn merge_or_fail(dir: &std::path::Path, branch: &str, msg: Option<&str>, ctx: &str) -> Result<()> {
    if let git::MergeResult::Conflict = git::git_merge(dir, branch, msg)? {
        return Err(BallError::Conflict(ctx.to_string()));
    }
    Ok(())
}

fn worktree_path(store: &Store, id: &str) -> Result<PathBuf> {
    task::validate_id(id)?;
    Ok(store.worktrees_root()?.join(id))
}

pub fn create_worktree(store: &Store, id: &str, identity: &str) -> Result<PathBuf> {
    // Quick existence check (no lock needed).
    if !store.task_exists(id) {
        return Err(BallError::TaskNotFound(id.to_string()));
    }

    with_task_lock(store, id, || {
        // All validation happens under the lock so two claims on the same
        // task can't both pass.
        let mut task = store.load_task(id)?;
        if task.status != Status::Open {
            return Err(BallError::NotClaimable(format!("{} (status = {})", id, task.status.as_str())));
        }
        if task.claimed_by.is_some() {
            return Err(BallError::AlreadyClaimed(id.to_string()));
        }

        let all = store.all_tasks()?;
        if crate::ready::is_dep_blocked(&all, &task) {
            return Err(BallError::DepsUnmet(id.to_string()));
        }

        let wt_path = worktree_path(store, id)?;
        if wt_path.exists() {
            return Err(BallError::WorktreeExists(wt_path));
        }
        if claim_file_path(store, id).exists() {
            return Err(BallError::AlreadyClaimed(id.to_string()));
        }

        let branch = format!("work/{}", id);
        task.status = Status::InProgress;
        task.claimed_by = Some(identity.to_string());
        task.branch = Some(branch.clone());
        task.touch();

        store.save_task(&task)?;
        store.commit_task(id, &format!("balls: claim {} - {}", id, task.title))?;

        if let Some(parent) = wt_path.parent() {
            fs::create_dir_all(parent)?;
        }

        git::git_worktree_add(&store.root, &wt_path, &branch).inspect_err(|_| {
            let _ = rollback_claim(store, id);
        })?;

        // Symlink .balls/local, .balls/worktree, .balls/tasks so the bl
        // worktree sees the same shared state as main: claims/locks,
        // the state-branch checkout, and the task symlink.
        let wt_balls = wt_path.join(".balls");
        fs::create_dir_all(&wt_balls)?;
        use std::os::unix::fs::symlink;
        let link = |src: PathBuf, name: &str| -> Result<()> {
            let dst = wt_balls.join(name);
            if !dst.exists() { symlink(src, dst)?; }
            Ok(())
        };
        link(store.local_dir(), "local")?;
        if !store.stealth {
            link(store.state_worktree_dir(), "worktree")?;
            link(PathBuf::from("worktree/.balls/tasks"), "tasks")?;
        }

        write_claim_file(store, id, identity)?;
        Ok(wt_path.clone())
    })
}

fn rollback_claim(store: &Store, id: &str) -> Result<()> {
    if let Ok(mut t) = store.load_task(id) {
        t.status = Status::Open;
        t.claimed_by = None;
        t.branch = None;
        t.touch();
        store.save_task(&t)?;
        let _ = store.commit_task(id, &format!("balls: rollback claim {}", id));
    }
    let _ = fs::remove_file(claim_file_path(store, id));
    Ok(())
}

/// Submit for review: commit the worker's code, squash-merge to main as
/// the single feature commit, flip task status to review on the state
/// branch. Keeps the worktree so a rejected review can be re-worked in
/// place.
pub fn review_worktree(store: &Store, id: &str, message: Option<&str>, identity: &str) -> Result<()> {
    let wt_path = worktree_path(store, id)?;
    let task = store.load_task(id)?;
    let branch = task.branch.clone().unwrap_or_else(|| format!("work/{}", id));

    with_task_lock(store, id, || {
        git::git_add_all(&wt_path)?;
        let _ = git::git_commit(&wt_path, &format!("wip: {}", id));
        let main_branch = git::git_current_branch(&store.root)?;
        merge_or_fail(
            &wt_path, &main_branch, None,
            &format!("conflicts merging {} into work/{}. Resolve in worktree, then retry.", main_branch, id),
        )?;

        // Flip the task to review on the state branch. store.task_path
        // points at the state worktree's copy, not the bl worktree's.
        let task_path = store.task_path(id)?;
        let mut t = Task::load(&task_path)?;
        t.status = Status::Review;
        t.touch();
        t.save(&task_path)?;
        if let Some(msg) = message {
            task_io::append_note_to(&task_path, identity, msg)?;
        }
        store.commit_task(id, &format!("state: review {}", id))?;

        // Squash merge the worker's branch into main. This is the single
        // substantive feature commit — the delivery tag [bl-XXXX] is
        // embedded so tooling and humans can trace main <-> state branch.
        let squash_msg = match message {
            Some(msg) => format!("{} [{}]", msg, id),
            None => format!("{} [{}]", task.title, id),
        };
        if let git::MergeResult::Conflict = git::git_merge_squash(&store.root, &branch)? {
            return Err(BallError::Conflict(format!(
                "unexpected conflict squash-merging {} into {}", branch, main_branch
            )));
        }
        git::git_commit(&store.root, &squash_msg)?;

        // Sync main back into worktree so re-review after rejection only
        // picks up new changes (squash merge doesn't record branch ancestry).
        let _ = git::git_merge(&wt_path, &main_branch, None);

        Ok(())
    })
}

/// Close a reviewed task: archive + remove worktree. Rejects from inside worktree.
pub fn close_worktree(store: &Store, id: &str, message: Option<&str>, identity: &str) -> Result<Task> {
    let wt_path = worktree_path(store, id)?;
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.starts_with(&wt_path) {
            return Err(BallError::Other(
                "cannot close from within the worktree — run from the repo root".into(),
            ));
        }
    }

    with_task_lock(store, id, || {
        let mut t = store.load_task(id)?;
        let branch = t.branch.clone().unwrap_or_else(|| format!("work/{}", id));
        t.status = Status::Closed;
        t.closed_at = Some(chrono::Utc::now());
        t.touch();
        store.save_task(&t)?;
        if let Some(msg) = message {
            let task_path = store.task_path(id)?;
            task_io::append_note_to(&task_path, identity, msg)?;
        }

        if wt_path.exists() {
            git::git_worktree_remove(&store.root, &wt_path, true)?;
        }
        let _ = git::git_branch_delete(&store.root, &branch, true);
        let _ = fs::remove_file(claim_file_path(store, id));

        // Archive stages deletions on the state branch; commit there, not main.
        archive_task(store, &t)?;
        store.commit_staged(&format!("state: close {} - {}", id, t.title))?;
        Ok(t)
    })
}

/// Stage task deletion and parent updates. Does NOT commit — caller is
/// responsible for committing all staged changes in one shot.
pub fn archive_task(store: &Store, task: &Task) -> Result<()> {
    use crate::task::ArchivedChild;
    let archived = ArchivedChild {
        id: task.id.clone(),
        title: task.title.clone(),
        closed_at: task.closed_at.unwrap_or_else(chrono::Utc::now),
    };

    // If this task has a parent, record the archived child on the parent
    if let Some(pid) = &task.parent {
        if let Ok(mut parent) = store.load_task(pid) {
            parent.closed_children.push(archived);
            parent.touch();
            store.save_task(&parent)?;
            if !store.stealth {
                let rel = PathBuf::from(format!(".balls/tasks/{}.json", pid));
                git::git_add(&store.state_worktree_dir(), &[rel.as_path()])?;
            }
        }
    }

    // Delete the closed task file (stages the git rm, but does not commit).
    store.delete_task_file(&task.id)?;
    store.rm_task_git(&task.id)?;
    Ok(())
}

pub fn drop_worktree(store: &Store, id: &str, force: bool) -> Result<()> {
    let wt_path = worktree_path(store, id)?;
    let task = store.load_task(id)?;
    let branch = task.branch.clone().unwrap_or_else(|| format!("work/{}", id));

    with_task_lock(store, id, || {
        if wt_path.exists() && !force && git::has_uncommitted_changes(&wt_path)? {
            return Err(BallError::Other(format!(
                "worktree {} has uncommitted changes. Use --force to drop.",
                wt_path.display()
            )));
        }

        // Reset task
        let mut t = store.load_task(id)?;
        let title = t.title.clone();
        t.status = Status::Open;
        t.claimed_by = None;
        t.branch = None;
        t.touch();
        store.save_task(&t)?;
        store.commit_task(id, &format!("balls: drop {} - {}", id, title))?;

        // Remove worktree (force because we may have uncommitted changes)
        if wt_path.exists() {
            git::git_worktree_remove(&store.root, &wt_path, true)?;
        }
        let _ = git::git_branch_delete(&store.root, &branch, true);

        let _ = fs::remove_file(claim_file_path(store, id));
        Ok(())
    })
}

pub fn cleanup_orphans(store: &Store) -> Result<(Vec<String>, Vec<String>)> {
    // Returns (removed_claims, removed_worktrees)
    let mut removed_claims = Vec::new();
    let mut removed_wts = Vec::new();
    let claims_dir = store.claims_dir();
    if claims_dir.exists() {
        for e in fs::read_dir(&claims_dir)? {
            let e = e?;
            let id = e.file_name().to_string_lossy().to_string();
            if !store.task_exists(&id) {
                let _ = fs::remove_file(e.path());
                removed_claims.push(id);
            }
        }
    }
    let wt_root = store.worktrees_root()?;
    if wt_root.exists() {
        for e in fs::read_dir(&wt_root)? {
            let e = e?;
            let id = e.file_name().to_string_lossy().to_string();
            if !claim_file_path(store, &id).exists() {
                let p = e.path();
                let _ = git::git_worktree_remove(&store.root, &p, true);
                removed_wts.push(id);
            }
        }
    }
    Ok((removed_claims, removed_wts))
}
