//! sync, resolve, prime, repair — remote reconciliation and agent bootstrap.

use super::{default_identity, discover};
use super::sync_report::apply_sync_report;
use balls::error::{BallError, Result};
use balls::store::Store;
use balls::task::{Status, Task};
use balls::{git, git_state, plugin, ready, resolve, worktree};
use std::fs;
use std::path::{Path, PathBuf};

pub fn cmd_sync(remote: String, task_filter: Option<String>) -> Result<()> {
    let store = discover()?;
    if git::git_has_remote(&store.root, &remote) {
        sync_with_remote(&store, &remote)?;
    }
    match plugin::run_plugin_sync(&store, task_filter.as_deref()) {
        Ok(reports) => {
            for (plugin_name, report) in reports {
                apply_sync_report(&store, &plugin_name, &report);
            }
        }
        Err(e) => eprintln!("warning: plugin sync failed: {}", e),
    }
    println!("sync complete");
    Ok(())
}

fn sync_with_remote(store: &Store, remote: &str) -> Result<()> {
    if !git::git_fetch(&store.root, remote)? {
        eprintln!("warning: fetch failed, continuing offline");
    }
    // Per SPEC §7.3 push order: state branch first, main second. If the
    // main push fails after the state push lands on the remote, the
    // next sync's half-push detector (below) surfaces the orphaned
    // state commit so the main push can be retried.
    if !store.stealth {
        sync_branch(&store.state_worktree_dir(), remote, "balls/tasks")?;
    }
    let main_branch = git::git_current_branch(&store.root)?;
    sync_branch(&store.root, remote, &main_branch)?;

    if !store.stealth {
        for id in detect_half_push(store)? {
            eprintln!(
                "warning: state branch records close for {0} but no `[{0}]` tag reachable from main",
                id
            );
        }
    }
    Ok(())
}

/// Fetch + merge + push a single branch in `dir`. Retries once on push
/// failure to tolerate a contemporaneous remote advance.
fn sync_branch(dir: &Path, remote: &str, branch: &str) -> Result<()> {
    let remote_ref = format!("{}/{}", remote, branch);
    fetch_merge_resolve_at(dir, remote, &remote_ref)?;
    if git::git_push(dir, remote, branch).is_err() {
        fetch_merge_resolve_at(dir, remote, &remote_ref)?;
        git::git_push(dir, remote, branch)?;
    }
    Ok(())
}

/// Scan the state branch for tasks that went through review (a
/// `state: review bl-xxxx` commit exists) and are now closed
/// (`state: close bl-xxxx`), but whose corresponding `[bl-xxxx]`
/// delivery tag is not reachable from main. Each hit is a half-push:
/// the state branch landed but the feature commit never did. Tasks
/// closed via `bl update status=closed` without ever being reviewed
/// are excluded — they legitimately never produce a main commit.
pub fn detect_half_push(store: &Store) -> Result<Vec<String>> {
    let state_dir = store.state_worktree_dir();
    let state_subjects = git_state::log_subjects(&state_dir, "balls/tasks")?;
    let reviewed: std::collections::HashSet<String> = state_subjects
        .iter()
        .filter_map(|s| extract_state_id(s, "state: review "))
        .collect();
    let main_branch = git::git_current_branch(&store.root)?;
    let main_subjects = git_state::log_subjects(&store.root, &main_branch)?;
    let mut missing = Vec::new();
    for subj in &state_subjects {
        let Some(id) = extract_state_id(subj, "state: close ") else { continue };
        if !reviewed.contains(&id) { continue; }
        let tag = format!("[{}]", id);
        if !main_subjects.iter().any(|s| s.contains(&tag)) && !missing.contains(&id) {
            missing.push(id);
        }
    }
    Ok(missing)
}

fn extract_state_id(subject: &str, prefix: &str) -> Option<String> {
    let rest = subject.strip_prefix(prefix)?;
    let id = rest.split_whitespace().next()?;
    id.starts_with("bl-").then(|| id.to_string())
}

/// Fetch from `remote` in `dir`, merge `remote_branch` into whatever HEAD
/// points at there, and auto-resolve task-file conflicts. Tolerates a
/// missing upstream branch (the "first push" case).
fn fetch_merge_resolve_at(dir: &Path, remote: &str, remote_branch: &str) -> Result<()> {
    let _ = git::git_fetch(dir, remote);
    match git::git_merge(dir, remote_branch) {
        Ok(git::MergeResult::Conflict) => {
            auto_resolve_conflicts_at(dir)?;
            git::git_commit(dir, "state: auto-resolve sync conflicts")?;
        }
        Ok(_) => {}
        Err(_) => {
            // Remote branch may not exist yet; that's fine.
        }
    }
    Ok(())
}

fn auto_resolve_conflicts_at(dir: &Path) -> Result<()> {
    for path in git::git_list_conflicted_files(dir)? {
        let rel = path.strip_prefix(dir).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();
        let rel_p = Path::new(&*rel_str).to_path_buf();
        let is_task = rel_str.starts_with(".balls/tasks/") && rel_str.ends_with(".json");
        let is_notes = rel_str.ends_with(".notes.jsonl");
        if !is_task && !is_notes {
            return Err(BallError::Conflict(format!(
                "unhandled conflict in {}",
                path.display()
            )));
        }
        if is_notes {
            // merge=union handles modify/modify; only delete/modify
            // reaches here, with the surviving side's content already
            // in the working tree. Stage as-is.
            git::git_add(dir, &[rel_p.as_path()])?;
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let (ours, theirs) = resolve::parse_conflict_markers(&content)?;
        let merged = resolve::resolve_conflict(&ours, &theirs);
        merged.save(&path)?;
        git::git_add(dir, &[rel_p.as_path()])?;
    }
    Ok(())
}

pub fn cmd_resolve(file: String) -> Result<()> {
    let path = PathBuf::from(&file);
    let content = fs::read_to_string(&path)?;
    let (ours, theirs) = resolve::parse_conflict_markers(&content)?;
    let merged = resolve::resolve_conflict(&ours, &theirs);
    merged.save(&path)?;
    println!("resolved {}", file);
    Ok(())
}

pub fn cmd_prime(identity: Option<String>, json: bool) -> Result<()> {
    let store = discover()?;
    let ident = identity.unwrap_or_else(default_identity);

    // Try to sync; ignore failure
    let _ = cmd_sync("origin".to_string(), None);

    let tasks = store.all_tasks()?;
    let ready_tasks = ready::ready_queue(&tasks);
    let claimed: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.claimed_by.as_deref() == Some(&ident))
        .filter(|t| t.status == Status::InProgress)
        .collect();

    if json {
        let obj = serde_json::json!({
            "identity": ident,
            "claimed": claimed,
            "ready": ready_tasks,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
        return Ok(());
    }

    println!("=== balls prime: {} ===", ident);
    for t in &claimed {
        let wt_dir = store
            .worktrees_root()
            .map(|r| r.join(&t.id))
            .unwrap_or_default();
        println!(
            "Claimed (resume): {} \"{}\" @ {}",
            t.id,
            t.title,
            wt_dir.display()
        );
    }
    println!("Ready:");
    for t in &ready_tasks {
        println!("  [P{}] {} \"{}\"", t.priority, t.id, t.title);
    }
    println!("===");
    Ok(())
}

pub fn cmd_repair(fix: bool) -> Result<()> {
    let store = discover()?;
    let dir = store.tasks_dir();
    let mut bad = Vec::new();
    if dir.exists() {
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Err(err) = Task::load(&p) {
                bad.push((p, err.to_string()));
            }
        }
    }
    if bad.is_empty() {
        println!("All task files OK.");
    } else {
        for (p, e) in &bad {
            println!("BAD: {} - {}", p.display(), e);
        }
    }
    if fix {
        let (rc, rw) = worktree::cleanup_orphans(&store)?;
        for id in &rc {
            println!("removed orphan claim: {}", id);
        }
        for id in &rw {
            println!("removed orphan worktree: {}", id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::extract_state_id;

    #[test]
    fn extract_state_id_handles_matching_prefix() {
        assert_eq!(
            extract_state_id("state: close bl-abcd - title", "state: close "),
            Some("bl-abcd".into())
        );
        assert_eq!(
            extract_state_id("state: review bl-1234", "state: review "),
            Some("bl-1234".into())
        );
    }

    #[test]
    fn extract_state_id_rejects_wrong_prefix() {
        assert!(extract_state_id("unrelated commit", "state: close ").is_none());
    }

    #[test]
    fn extract_state_id_rejects_non_task_id() {
        // A subject with the right prefix but an id that doesn't start
        // with `bl-` must not be mistaken for a task reference.
        assert!(extract_state_id("state: close custom foo", "state: close ").is_none());
    }

    #[test]
    fn extract_state_id_rejects_empty_tail() {
        assert!(extract_state_id("state: close ", "state: close ").is_none());
    }
}
