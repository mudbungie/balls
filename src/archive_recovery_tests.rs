//! Unit coverage for `archive_recovery`. Pure helpers
//! (`split_sha_date`, `id_from_path`, `parse_deletions`) are exercised
//! with crafted input so the recreate/live/header-corruption branches
//! that real git never emits are still covered; the git-touching
//! functions run against a real temp store.

use super::*;
use crate::store::Store;
use crate::task::{NewTaskOpts, Status, Task};
use chrono::Utc;
use std::path::Path;
use tempfile::tempdir;

fn raw_git(dir: &Path, args: &[&str]) {
    assert!(
        clean_git_command(dir).args(args).output().unwrap().status.success(),
        "git {args:?}"
    );
}

fn init_store(dir: &Path) -> Store {
    raw_git(dir, &["init", "-q", "-b", "main"]);
    raw_git(dir, &["config", "user.email", "t@e.x"]);
    raw_git(dir, &["config", "user.name", "t"]);
    raw_git(dir, &["config", "commit.gpgsign", "false"]);
    Store::init(dir, false, None).unwrap()
}

/// A store where recovery is impossible: no-git + stealth (the
/// `--tasks-dir` outside-a-repo mode). `available()` is false.
fn unavailable_store(dir: &Path) -> Store {
    let td = dir.join("tasks").to_string_lossy().into_owned();
    let s = Store::init(dir, false, Some(td)).unwrap();
    assert!(s.no_git && s.stealth);
    s
}

fn archive(store: &Store, id: &str, title: &str) {
    let mut t = Task::new(
        NewTaskOpts { title: title.into(), ..Default::default() },
        id.into(),
    );
    t.status = Status::Review;
    store.save_task(&t).unwrap();
    store.commit_task(id, &format!("state: review {id}")).unwrap();
    let mut closed = t.clone();
    closed.status = Status::Closed;
    closed.closed_at = Some(Utc::now());
    store
        .close_and_archive(&closed, &format!("state: close {id} - {title}"))
        .unwrap();
}

// --- available -----------------------------------------------------

#[test]
fn available_true_for_normal_store_false_for_no_git() {
    let td = tempdir().unwrap();
    assert!(available(&init_store(td.path())));
    let plain = tempdir().unwrap();
    assert!(!available(&unavailable_store(plain.path())));
}

// --- git_out -------------------------------------------------------

#[test]
fn git_out_spawn_failure_is_none() {
    assert!(git_out(Path::new("/no/such/dir/xyz"), &["status"]).is_none());
}

#[test]
fn git_out_success_and_nonzero() {
    let td = tempdir().unwrap();
    let store = init_store(td.path());
    let sw = store.state_worktree_dir();
    assert!(git_out(&sw, &["rev-parse", "HEAD"]).is_some());
    assert!(git_out(&sw, &["cat-file", "-e", "deadbeefdeadbeef"]).is_none());
}

// --- split_sha_date ------------------------------------------------

#[test]
fn split_sha_date_cases() {
    let (sha, _) = split_sha_date("ab\t2026-05-18T00:00:00+00:00").unwrap();
    assert_eq!(sha, "ab");
    assert!(split_sha_date("notab").is_none());
    assert!(split_sha_date("ab\tnotadate").is_none());
}

// --- id_from_path --------------------------------------------------

#[test]
fn id_from_path_cases() {
    assert_eq!(id_from_path(".balls/tasks/bl-aaaa.json"), Some("bl-aaaa"));
    assert_eq!(id_from_path(".balls/tasks/bl-aaaa.notes.jsonl"), None);
    assert_eq!(id_from_path("src/main.rs"), None);
    assert_eq!(id_from_path(".balls/tasks/notbl.json"), None);
}

// --- parse_deletions ----------------------------------------------

#[test]
fn parse_deletions_dedupes_skips_notes_and_sorts() {
    let log = "\u{1}s1\t2026-05-02T00:00:00+00:00\n\
               .balls/tasks/bl-bbbb.json\n\
               .balls/tasks/bl-bbbb.notes.jsonl\n\
               \u{1}s2\t2026-05-01T00:00:00+00:00\n\
               .balls/tasks/bl-aaaa.json\n\
               .balls/tasks/bl-bbbb.json\n\
               .balls/tasks/.gitkeep\n\
               \u{1}s3\t2026-05-01T00:00:00+00:00\n\
               .balls/tasks/bl-cccc.json\n";
    let got = parse_deletions(log, |_| false);
    let ids: Vec<&str> = got.iter().map(|(i, ..)| i.as_str()).collect();
    let shas: Vec<&str> = got.iter().map(|(_, s, _)| s.as_str()).collect();
    // 05-01 ties broken by id (aaaa<cccc), bbbb (05-02) last.
    assert_eq!(ids, ["bl-aaaa", "bl-cccc", "bl-bbbb"]);
    assert_eq!(shas, ["s2", "s3", "s1"]);
}

#[test]
fn parse_deletions_skips_path_without_valid_header() {
    let log = ".balls/tasks/bl-early.json\n\
               \u{1}badheader-no-tab\n\
               .balls/tasks/bl-xxxx.json\n\
               \u{1}s9\tnotadate\n\
               .balls/tasks/bl-yyyy.json\n";
    assert!(parse_deletions(log, |_| false).is_empty());
}

#[test]
fn parse_deletions_skips_live_ids() {
    let log = "\u{1}s1\t2026-05-01T00:00:00+00:00\n\
               .balls/tasks/bl-live.json\n\
               .balls/tasks/bl-keep.json\n";
    let got = parse_deletions(log, |id| id == "bl-live");
    assert_eq!(got.iter().map(|(i, ..)| i.as_str()).collect::<Vec<_>>(), ["bl-keep"]);
}

// --- task_at_predeletion ------------------------------------------

#[test]
fn task_at_predeletion_bogus_sha_is_none() {
    let td = tempdir().unwrap();
    let store = init_store(td.path());
    let zero = "0".repeat(40);
    assert!(
        task_at_predeletion(&store.state_worktree_dir(), "bl-aaaa", &zero, Utc::now())
            .is_none()
    );
}

// --- recover_one ---------------------------------------------------

#[test]
fn recover_one_reconstructs_closed_task() {
    let td = tempdir().unwrap();
    let store = init_store(td.path());
    archive(&store, "bl-aaaa", "Title A");
    let t = recover_one(&store, "bl-aaaa").unwrap();
    assert_eq!(t.id, "bl-aaaa");
    assert_eq!(t.title, "Title A");
    assert_eq!(t.status, Status::Closed);
    assert!(t.closed_at.is_some());
}

#[test]
fn recover_one_none_for_never_closed_and_unavailable() {
    let td = tempdir().unwrap();
    let store = init_store(td.path());
    assert!(recover_one(&store, "bl-ffff").is_none());
    let plain = tempdir().unwrap();
    assert!(recover_one(&unavailable_store(plain.path()), "bl-aaaa").is_none());
}

// --- recover_all ---------------------------------------------------

#[test]
fn recover_all_empty_when_unavailable() {
    let plain = tempdir().unwrap();
    assert!(recover_all(&unavailable_store(plain.path())).is_empty());
}

#[test]
fn recover_all_lists_closed_and_drops_unparseable() {
    let td = tempdir().unwrap();
    let store = init_store(td.path());
    archive(&store, "bl-aaaa", "A");
    archive(&store, "bl-bbbb", "B");
    // A non-JSON archived file: parse_deletions yields it, but
    // task_at_predeletion fails to parse, so filter_map drops it.
    let sw = store.state_worktree_dir();
    std::fs::write(sw.join(".balls/tasks/bl-junk.json"), "not json").unwrap();
    raw_git(&sw, &["add", ".balls/tasks/bl-junk.json"]);
    raw_git(&sw, &["commit", "-m", "add junk", "--no-verify"]);
    raw_git(&sw, &["rm", "-f", ".balls/tasks/bl-junk.json"]);
    raw_git(&sw, &["commit", "-m", "state: close bl-junk", "--no-verify"]);

    let got = recover_all(&store);
    let ids: Vec<&str> = got.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["bl-aaaa", "bl-bbbb"]);
    assert!(got.iter().all(|t| t.status == Status::Closed));
}
