//! Wire-classification tests for the git-remote `Protocol` impl.
//! The retry-loop state machine is covered in `negotiation_tests.rs`
//! against a synthetic Protocol, so this file only exercises the
//! git-stderr classifier and the `is_unreachable` marker set.

use super::*;
use crate::negotiation::Protocol;
use crate::task::{NewTaskOpts, Task};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use tempfile::tempdir;

fn out(success: bool, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(if success { 0 } else { 1 << 8 }),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn classify_success() {
    assert_eq!(classify_push_output(&out(true, "")), AttemptClass::Ok);
}

#[test]
fn classify_non_ff_rejected() {
    let s = "! [rejected] balls/tasks -> balls/tasks (non-fast-forward)";
    assert_eq!(classify_push_output(&out(false, s)), AttemptClass::Conflict);
}

#[test]
fn classify_fetch_first_rejected() {
    let s = "! [rejected] (fetch first)";
    assert_eq!(classify_push_output(&out(false, s)), AttemptClass::Conflict);
}

#[test]
fn classify_bracket_rejected_alone() {
    // Hits the third arm of the OR: stderr has "rejected" + "[rejected]"
    // but neither "non-fast-forward" nor "fetch first".
    let s = "warning: failed; rejected with [rejected] tag";
    assert_eq!(classify_push_output(&out(false, s)), AttemptClass::Conflict);
}

#[test]
fn classify_unreachable_dns() {
    let s = "fatal: Could not resolve hostname github.com";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        AttemptClass::Unreachable(_)
    ));
}

#[test]
fn classify_unreachable_repo_not_found() {
    let s = "ERROR: Repository not found.";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        AttemptClass::Unreachable(_)
    ));
}

#[test]
fn classify_other_falls_through() {
    let s = "fatal: weird unexpected error from git";
    assert!(matches!(
        classify_push_output(&out(false, s)),
        AttemptClass::Other(_)
    ));
}

#[test]
fn is_unreachable_recognises_common_markers() {
    assert!(is_unreachable("connection refused"));
    assert!(is_unreachable("permission denied"));
    assert!(is_unreachable("network is unreachable"));
    assert!(!is_unreachable("totally fine"));
}

/// Stand up a stealth, no-git Store with a single task whose
/// `claimed_by` is `claimer`. Stealth+no-git skips the state-branch
/// machinery so `post_merge` can be exercised without a real remote.
fn stealth_store_with_task(claimer: &str) -> (tempfile::TempDir, Store, String) {
    let td = tempdir().unwrap();
    let tasks_dir = td.path().join("tasks");
    let store = Store::init(
        td.path(),
        true,
        Some(tasks_dir.to_string_lossy().into_owned()),
    )
    .unwrap();
    let mut task = Task::new(NewTaskOpts { title: "t".into(), ..Default::default() }, "bl-7e57".into());
    task.claimed_by = Some(claimer.into());
    store.save_task(&task).unwrap();
    (td, store, "bl-7e57".into())
}

#[test]
fn post_merge_returns_none_when_we_still_own_claim() {
    let (_td, store, id) = stealth_store_with_task("alice");
    let mut p = GitRemoteClaimProtocol::new(&store, &id, "alice");
    assert!(p.post_merge().unwrap().is_none());
}

#[test]
fn post_merge_returns_lost_with_unknown_winner_when_claim_cleared() {
    // Claimer field empty (someone else's merge cleared it) -> Lost
    // with the "(unknown)" placeholder. Push attempted best-effort;
    // it'll fail in this no-git fixture, which is the path we want
    // to exercise.
    let (_td, store, id) = stealth_store_with_task("");
    let mut task = store.load_task(&id).unwrap();
    task.claimed_by = None;
    store.save_task(&task).unwrap();
    let mut p = GitRemoteClaimProtocol::new(&store, &id, "alice");
    let outcome = p.post_merge().unwrap().unwrap();
    assert_eq!(outcome, SyncedClaimResult::Lost { winner: "(unknown)".into() });
}

// --- GitRemoteParticipant trait surface -------------------------------
//
// End-to-end claim-side wiring is exercised by `tests/claim_sync.rs`
// against a real remote. Here we cover the participant metadata and
// the per-event branches that a Claim-only integration test cannot
// reach (events the participant explicitly does not yet wire).

use crate::participant::{Event, Participant, Projection};

#[test]
fn participant_advertises_git_remote_name_and_full_projection() {
    let p = GitRemoteParticipant::for_claim();
    assert_eq!(p.name(), "git-remote");
    assert_eq!(p.subscriptions(), &[Event::Claim]);
    assert_eq!(*p.projection(), Projection::full());
}

#[test]
fn participant_default_matches_for_claim() {
    let a = GitRemoteParticipant::default();
    let b = GitRemoteParticipant::for_claim();
    assert_eq!(a.subscriptions(), b.subscriptions());
    assert_eq!(*a.projection(), *b.projection());
}

#[test]
fn participant_failure_policy_is_required_only_on_claim() {
    let p = GitRemoteParticipant::for_claim();
    assert_eq!(p.failure_policy(Event::Claim), FailurePolicy::Required);
    // Non-claim events fall through to BestEffort. bl-2bf7 wires
    // review/close per-event policy from config; until then the
    // participant just declares the safe default.
    for e in [Event::Review, Event::Close, Event::Update, Event::Sync] {
        assert_eq!(p.failure_policy(e), FailurePolicy::BestEffort);
    }
}

#[test]
fn participant_protocol_is_some_only_for_claim() {
    let (_td, store, id) = stealth_store_with_task("alice");
    let p = GitRemoteParticipant::for_claim();
    let mk = |event| crate::participant::EventCtx {
        event,
        store: &store,
        task_id: &id,
        identity: "alice",
    };
    assert!(p.protocol(Event::Claim, mk(Event::Claim)).is_some());
    for e in [Event::Review, Event::Close, Event::Update, Event::Sync] {
        assert!(p.protocol(e, mk(e)).is_none());
    }
}
