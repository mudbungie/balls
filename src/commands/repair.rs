//! `bl repair` — half-push retraction, task-file scan, orphan cleanup,
//! moved-clone rebind.

use super::discover;
use super::half_push::{detect_half_push, write_forget_half_push};
use balls::error::{BallError, Result};
use balls::task::Task;
use balls::worktree;
use std::fmt::Write;
use std::fs;

/// All CLI flags passed to `bl repair`. Bundled into a struct so the
/// dispatcher in `main.rs` stays under the readable-arg-count and the
/// command signature can grow Phase 4+ subcommands without re-touching
/// every call site.
pub struct RepairArgs {
    pub fix: bool,
    pub forget_half_push: Vec<String>,
    pub forget_all_half_pushes: bool,
    pub rebind_path: bool,
}

pub fn cmd_repair(args: RepairArgs) -> Result<()> {
    let RepairArgs {
        fix,
        forget_half_push,
        forget_all_half_pushes,
        rebind_path,
    } = args;
    let store = discover()?;

    // Phase 3 (bl-05e5): the moved-clone rebind is its own action
    // (one move per orphan, no scan output) and exits early — running
    // it alongside the half-push retraction or the task-file scan
    // would be a confusing UX.
    if rebind_path {
        return run_rebind(&store);
    }

    // Forget actions run first: they're surgical and only touch the
    // state branch. They don't depend on, or interact with, the
    // task-file scan or orphan-cleanup paths below.
    if !forget_half_push.is_empty() || forget_all_half_pushes {
        if store.no_git || store.stealth {
            return Err(BallError::Other(
                "--forget-half-push requires a non-stealth git-backed repo".into(),
            ));
        }
        let flagged = detect_half_push(&store)?;
        let targets: Vec<String> = if forget_all_half_pushes {
            flagged.clone()
        } else {
            for id in &forget_half_push {
                if !flagged.contains(id) {
                    return Err(BallError::Other(format!(
                        "{id} is not a currently-flagged half-push; nothing to forget"
                    )));
                }
            }
            forget_half_push.clone()
        };
        if targets.is_empty() {
            println!("No half-push warnings to forget.");
        } else {
            write_forget_half_push(&store, &targets)?;
            for id in &targets {
                println!("forgot half-push: {id}");
            }
        }
        return Ok(());
    }

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
    if fix && !store.no_git {
        let (rc, rw) = worktree::cleanup_orphans(&store)?;
        for id in &rc {
            println!("removed orphan claim: {id}");
        }
        for id in &rw {
            println!("removed orphan worktree: {id}");
        }
    }
    Ok(())
}

/// Phase 3 (bl-05e5): drive [`balls::repair_rebind::run`] from the
/// CLI. Each rebound orphan prints one line per moved sibling; an
/// empty orphan list is reported explicitly so the user knows doctor
/// would have nothing to say either.
fn run_rebind(store: &balls::store::Store) -> Result<()> {
    reject_unsupported_for_rebind(store.no_git, store.stealth)?;
    let reports = balls::repair_rebind::run(&store.root)?;
    print!("{}", format_rebind_reports(&reports));
    Ok(())
}

/// Pure check: `--rebind-path` requires a non-stealth XDG-mode clone.
/// Pulled out so the error branch has a direct test (the integration
/// path would need a stealth clone fixture purely for one error
/// branch).
pub(crate) fn reject_unsupported_for_rebind(no_git: bool, stealth: bool) -> Result<()> {
    if no_git || stealth {
        return Err(BallError::Other(
            "--rebind-path requires a non-stealth XDG-mode clone".into(),
        ));
    }
    Ok(())
}

/// Pure: format the rebind reports the user sees on stdout. Empty
/// input renders as the "nothing to rebind" line; populated input
/// renders one block per orphan.
#[must_use]
pub(crate) fn format_rebind_reports(reports: &[balls::repair_rebind::RebindReport]) -> String {
    let mut out = String::new();
    if reports.is_empty() {
        out.push_str("nothing to rebind; no orphaned per-clone state for this clone\n");
        return out;
    }
    for r in reports {
        let _ = writeln!(out, "rebound {} → {}", r.nested_from.display(), r.nested_to.display());
        for p in &r.moved {
            let _ = writeln!(out, "  moved: {}", p.display());
        }
    }
    out
}

#[cfg(test)]
mod rebind_tests {
    use super::{format_rebind_reports, reject_unsupported_for_rebind};
    use balls::repair_rebind::RebindReport;
    use std::path::PathBuf;

    #[test]
    fn no_git_rejected() {
        assert!(reject_unsupported_for_rebind(true, false).is_err());
    }

    #[test]
    fn stealth_rejected() {
        assert!(reject_unsupported_for_rebind(false, true).is_err());
    }

    #[test]
    fn xdg_with_git_accepted() {
        assert!(reject_unsupported_for_rebind(false, false).is_ok());
    }

    #[test]
    fn empty_reports_render_nothing_to_rebind() {
        let out = format_rebind_reports(&[]);
        assert!(out.contains("nothing to rebind"));
    }

    #[test]
    fn populated_reports_render_one_block_per_orphan() {
        let r = RebindReport {
            nested_from: PathBuf::from("home/u/old"),
            nested_to: PathBuf::from("home/u/new"),
            moved: vec![PathBuf::from("/x/y/claims/home/u/new")],
        };
        let out = format_rebind_reports(std::slice::from_ref(&r));
        assert!(out.contains("rebound home/u/old → home/u/new"));
        assert!(out.contains("moved: /x/y/claims/home/u/new"));
    }
}
