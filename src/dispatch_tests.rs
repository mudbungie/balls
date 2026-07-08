//! Tests for the §8 dispatch entrypoint [`crate::run`] — verb resolution and
//! the per-class wiring (prime/sync, mutate, reads) through the real engine.

use super::support::*;
use tempfile::TempDir;

#[test]
fn install_dispatches_to_its_run_wiring() {
    // The verb is wired (§6): before prime it reports the missing checkout
    // (exit 1, like any op), not a placeholder plan. The full seal path is
    // covered in `install_run_tests` / `tests/dispatch.rs`.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["install", "--from", "balls/tasks"]), 1);
}

#[test]
fn a_read_verb_renders_the_store_and_exits_zero() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    // The reads dispatch through `reads::run` against the store (the old `ready`
    // verb is now `list --status ready`, §9).
    for a in [&["list"][..], &["list", "--status", "ready"], &["show", &id]] {
        assert_eq!(run_in(&tmp, a), 0);
    }
    // A read before prime is empty (§13); a missing id errors.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["list"]), 0);
    assert_eq!(run_in(&tmp, &["show", "bl-nope"]), 1);
}

#[test]
fn a_mutating_verb_before_prime_is_an_error() {
    // No landing yet — a deliverable op never bootstraps, it reports the miss.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["create", "A task"]), 1);
}

#[test]
fn create_claim_update_close_round_trip_through_the_engine() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    // create seals a fresh ball file onto the STORE.
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let tasks = store(&tmp).join("tasks");
    let id = sole_task_id(&tasks);
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["update", &id, "state=doing", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "me"]), 0);
    // close retires the file; the store has advanced past it.
    assert!(!tasks.join(format!("{id}.md")).exists());
}

#[test]
fn create_stamps_the_repo_root_and_claim_accepts_the_matching_checkout() {
    // bl-1ce7 end to end: when the invocation path IS a code repo, `create`
    // stamps its root-commit on the ball; a `claim` from that same checkout
    // matches and seals.
    let tmp = TempDir::new().unwrap();
    let root = git_root(&tmp, "one\n");
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let tasks = store(&tmp).join("tasks");
    let id = sole_task_id(&tasks);
    let md = std::fs::read_to_string(tasks.join(format!("{id}.md"))).unwrap();
    assert!(md.contains(&format!("root_commit = \"{root}\"")), "frontmatter records the root:\n{md}");
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "me"]), 0);
}

#[test]
fn claim_rejects_a_ball_created_against_a_different_repo_root() {
    // The wrong-repo guard: a ball stamped against root R1, claimed from a
    // checkout re-rooted to R2 (a history rewrite, or simply the wrong repo at
    // this path), is refused (exit 1) — no override.
    let tmp = TempDir::new().unwrap();
    git_root(&tmp, "one\n"); // R1, stamped on the ball at create
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    std::fs::remove_dir_all(tmp.path().join("proj").join(".git")).unwrap();
    git_root(&tmp, "two\n"); // R2 ≠ R1 (distinct seed ⇒ distinct root)
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "me"]), 1);
}

#[test]
fn subtask_of_claim_gates_the_epic_and_close_notices_open_children() {
    // §10/bl-5d9a: --subtask-of mints the parent + CLAIM-gate through the real
    // engine, so an epic with an open subtask is *blocked* — it refuses claim
    // and drops out of the ready set (a context-free agent can't land on an
    // unactionable container). close is NOT gated (the close-gate was dropped:
    // no lifecycle enforcement), and a close leaving non-gating children still
    // emits the §10 notice (the n > 0 stderr branch in `mutate::report::emit`).
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    let tasks = store(&tmp).join("tasks");
    let new_id = |known: &[&str]| -> String {
        std::fs::read_dir(&tasks)
            .unwrap()
            .filter_map(|e| e.unwrap().file_name().to_string_lossy().strip_suffix(".md").map(str::to_string))
            .find(|id| !known.contains(&id.as_str()))
            .unwrap()
    };
    assert_eq!(run_in(&tmp, &["create", "Epic", "--as", "me"]), 0);
    let epic = new_id(&[]);
    assert_eq!(run_in(&tmp, &["create", "Child", "--parent", &epic, "--as", "me"]), 0);
    let child = new_id(&[&epic]);
    assert_eq!(run_in(&tmp, &["create", "Gate", "--subtask-of", &epic, "--as", "me"]), 0);
    let gate = new_id(&[&epic, &child]);
    // The sugar's claim-gate holds end to end: the epic refuses to be CLAIMED.
    assert_eq!(run_in(&tmp, &["claim", &epic, "--as", "me"]), 1);
    // Close the gate (a ready leaf); its claim-blocker resolves by file-absence,
    // so the epic auto-readies and now claims with no manual edge teardown.
    assert_eq!(run_in(&tmp, &["close", &gate, "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["claim", &epic, "--as", "me"]), 0);
    // close is ungated: the epic closes over its still-open child, noticing it.
    assert_eq!(run_in(&tmp, &["close", &epic, "--as", "me"]), 0);
    assert!(!tasks.join(format!("{epic}.md")).exists());
    // The child survives, its parent pointer dangling (display-only, §3).
    assert!(tasks.join(format!("{child}.md")).exists());
}

#[test]
fn prime_founds_a_landing_then_converges_on_re_run() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert!(landing(&tmp).join("config").join("balls.toml").is_file());
    assert!(store(&tmp).join("tasks").is_dir());
    // Idempotent: a second prime is a no-op-converge, not an error (§12).
    assert_eq!(run_in(&tmp, &["prime"]), 0);
}

#[test]
fn sync_before_prime_is_an_error() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["sync"]), 1);
}

#[test]
fn sync_after_prime_targets_the_store() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime"]), 0);
    // Stealth store; the empty sync chain converges.
    assert_eq!(run_in(&tmp, &["sync"]), 0);
    assert_eq!(run_in(&tmp, &["sync", "landing"]), 0); // landing is never a target
}

#[test]
fn a_bad_flag_is_an_op_error() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["prime", "--center"]), 1);
}

#[test]
fn the_op_instant_dates_both_the_frontmatter_and_the_store_seal() {
    // bl-8b98 SSOT: with the clock pinned to T, the frontmatter ints AND the
    // store commit's author+committer dates all derive from the SAME instant —
    // no longer three independent reads that agree by luck.
    let tmp = TempDir::new().unwrap();
    let t = 1_700_000_000;
    assert_eq!(run_clocked(&tmp, t, &["prime", "--as", "me"]), 0);
    assert_eq!(run_clocked(&tmp, t, &["create", "A task", "--as", "me"]), 0);
    let tasks = store(&tmp).join("tasks");
    let id = sole_task_id(&tasks);
    let md = std::fs::read_to_string(tasks.join(format!("{id}.md"))).unwrap();
    assert!(md.contains("created = 1700000000"), "frontmatter created not T: {md}");
    assert!(md.contains("updated = 1700000000"), "frontmatter updated not T: {md}");
    // %at (author) and %ct (committer) on the store tip both read T (unix secs).
    let dates = crate::git::run(&store(&tmp), &["log", "-1", "--format=%at%n%ct"], None).unwrap();
    for line in dates.lines() {
        assert_eq!(line, "1700000000", "store seal date not T: {dates}");
    }
}

#[test]
fn a_named_but_unbound_clock_provider_falls_open_and_logs_the_note() {
    // Fail-open (§8): a configured provider with no bound bin degrades to the
    // system clock — the op still succeeds, and the note lands in the op log
    // (threshold-gated + persisted, not a bare stderr line).
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["conf", "set", "clock-provider", "ghost"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0); // non-fatal
    let log = std::fs::read_to_string(op_log(&tmp)).unwrap();
    assert!(log.contains("clock_provider ghost not bound"), "note missing from op log: {log}");
}
