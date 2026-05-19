//! `Participant`/`Protocol` wiring tests for the git-remote
//! state-branch push. The push-output classifier is covered in
//! `claim_push_tests.rs`; the retry-loop state machine is covered in
//! `negotiation_tests.rs` against a synthetic Protocol. This file
//! exercises the per-event `post_merge` ownership check and the
//! `GitRemoteParticipant` trait surface.

use super::*;
use crate::negotiation::Protocol;
use crate::task::{NewTaskOpts, Task};
use tempfile::tempdir;

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
    let mut p = GitRemoteClaimProtocol::new(Event::Claim, &store, &id, "alice");
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
    let mut p = GitRemoteClaimProtocol::new(Event::Claim, &store, &id, "alice");
    let outcome = p.post_merge().unwrap().unwrap();
    assert_eq!(outcome, SyncedClaimResult::Lost { winner: "(unknown)".into() });
}

#[test]
fn post_merge_skips_ownership_check_for_review_and_close() {
    // bl-2bf7: review/close don't carry a "lost" outcome — once the
    // field-level merge resolves any divergence, the loop just
    // retries the push. `post_merge` must return None even when the
    // local task no longer carries our identity in `claimed_by`.
    let (_td, store, id) = stealth_store_with_task("");
    let mut task = store.load_task(&id).unwrap();
    task.claimed_by = None;
    store.save_task(&task).unwrap();
    for ev in [Event::Review, Event::Close] {
        let mut p = GitRemoteClaimProtocol::new(ev, &store, &id, "alice");
        assert!(
            p.post_merge().unwrap().is_none(),
            "{ev:?} post_merge should never short-circuit"
        );
    }
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
fn participant_failure_policy_required_for_subscribed_events() {
    // The for_claim() shape stays Required-on-claim only.
    let p = GitRemoteParticipant::for_claim();
    assert_eq!(p.failure_policy(Event::Claim), FailurePolicy::Required);
    for e in [Event::Review, Event::Close, Event::Update, Event::Sync] {
        assert_eq!(p.failure_policy(e), FailurePolicy::BestEffort);
    }
    // Lifecycle subscriptions get Required on every event in the
    // declared set — the call site only constructs the participant
    // when policy says the remote must succeed for that transition.
    let p = GitRemoteParticipant::for_lifecycle(&[Event::Review, Event::Close]);
    assert_eq!(p.failure_policy(Event::Review), FailurePolicy::Required);
    assert_eq!(p.failure_policy(Event::Close), FailurePolicy::Required);
    assert_eq!(p.failure_policy(Event::Claim), FailurePolicy::BestEffort);
}

#[test]
fn participant_protocol_is_some_for_lifecycle_events() {
    let (_td, store, id) = stealth_store_with_task("alice");
    // for_claim() answers Some only on Claim — the bl-2148 surface
    // is unchanged for callers that opted into claim-only.
    let p = GitRemoteParticipant::for_claim();
    let mk = |event| crate::participant::EventCtx {
        event,
        store: &store,
        task_id: &id,
        identity: "alice",
    };
    assert!(p.protocol(Event::Claim, mk(Event::Claim)).is_some());
    // The protocol method itself answers Some for any state-branch
    // lifecycle event regardless of subscription — `participant::run`
    // gates on `subscriptions()` first, so this is never reached for
    // events the participant didn't opt into. Update/Sync stay None
    // because the wire (state-branch push) doesn't apply.
    for e in [Event::Review, Event::Close] {
        assert!(p.protocol(e, mk(e)).is_some(), "{e:?}");
    }
    for e in [Event::Update, Event::Sync] {
        assert!(p.protocol(e, mk(e)).is_none(), "{e:?}");
    }
}
