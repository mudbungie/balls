//! Primitive-level coverage. A `ClosureProtocol` lets every test
//! synthesize a wire from three closures; the loop's behavior is
//! verified independent of any real protocol.

use super::*;
use std::cell::RefCell;

type PropFn<'a> = Box<dyn FnMut() -> Result<AttemptClass> + 'a>;
type FetchFn<'a> = Box<dyn FnMut() -> Result<()> + 'a>;
type PostMergeFn<'a> = Box<dyn FnMut() -> Result<Option<&'static str>> + 'a>;

struct ClosureProtocol<'a> {
    propose: PropFn<'a>,
    fetch: FetchFn<'a>,
    post_merge: PostMergeFn<'a>,
    budget: usize,
    policy: CommitPolicy,
}

impl<'a> ClosureProtocol<'a> {
    fn new(propose: PropFn<'a>, budget: usize) -> Self {
        Self {
            propose,
            fetch: Box::new(|| Ok(())),
            post_merge: Box::new(|| Ok(None)),
            budget,
            policy: CommitPolicy::default(),
        }
    }
    fn with_fetch(mut self, f: FetchFn<'a>) -> Self {
        self.fetch = f;
        self
    }
    fn with_post_merge(mut self, f: PostMergeFn<'a>) -> Self {
        self.post_merge = f;
        self
    }
    fn with_policy(mut self, p: CommitPolicy) -> Self {
        self.policy = p;
        self
    }
}

impl Protocol for ClosureProtocol<'_> {
    type Outcome = &'static str;
    fn propose(&mut self) -> Result<AttemptClass> {
        (self.propose)()
    }
    fn fetch_remote_view(&mut self) -> Result<()> {
        (self.fetch)()
    }
    fn post_merge(&mut self) -> Result<Option<Self::Outcome>> {
        (self.post_merge)()
    }
    fn pushed(&mut self) -> Self::Outcome {
        "pushed"
    }
    fn retry_budget(&self) -> usize {
        self.budget
    }
    fn commit_policy(&self) -> CommitPolicy {
        self.policy.clone()
    }
}

fn always_ok<'a>() -> PropFn<'a> {
    Box::new(|| Ok(AttemptClass::Ok))
}

fn ok_default<O>(outcome: O) -> NegotiationResult<O> {
    NegotiationResult::Ok(Accepted {
        outcome,
        commit_policy: CommitPolicy::default(),
    })
}

#[test]
fn ok_first_attempt_returns_outcome() {
    let p = ClosureProtocol::new(always_ok(), 3);
    let r = Negotiation::new(p, FailurePolicy::Required).run().unwrap();
    assert_eq!(r, ok_default("pushed"));
}

#[test]
fn conflict_then_ok_retries_and_succeeds() {
    let calls = RefCell::new(0);
    let propose: PropFn = Box::new(|| {
        let mut c = calls.borrow_mut();
        *c += 1;
        if *c == 1 { Ok(AttemptClass::Conflict) } else { Ok(AttemptClass::Ok) }
    });
    let p = ClosureProtocol::new(propose, 5);
    let r = Negotiation::new(p, FailurePolicy::Required).run().unwrap();
    assert_eq!(r, ok_default("pushed"));
    assert_eq!(*calls.borrow(), 2);
}

#[test]
fn post_merge_short_circuits_to_outcome() {
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 5)
        .with_post_merge(Box::new(|| Ok(Some("lost"))));
    let r = Negotiation::new(p, FailurePolicy::Required).run().unwrap();
    assert_eq!(r, ok_default("lost"));
}

#[test]
fn ok_carries_protocol_chosen_commit_policy() {
    // ClosureProtocol's commit_policy is settable; Negotiation::run
    // copies it into Accepted on every Ok path (clean push and
    // post_merge short-circuit alike).
    let p = ClosureProtocol::new(always_ok(), 1).with_policy(CommitPolicy::Suppress);
    let r = Negotiation::new(p, FailurePolicy::Required).run().unwrap();
    assert_eq!(
        r,
        NegotiationResult::Ok(Accepted {
            outcome: "pushed",
            commit_policy: CommitPolicy::Suppress,
        })
    );
}

#[test]
fn post_merge_short_circuit_carries_commit_policy() {
    let policy = CommitPolicy::Batch { tag: "audit".into() };
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 5)
        .with_post_merge(Box::new(|| Ok(Some("lost"))))
        .with_policy(policy.clone());
    let r = Negotiation::new(p, FailurePolicy::Required).run().unwrap();
    assert_eq!(
        r,
        NegotiationResult::Ok(Accepted { outcome: "lost", commit_policy: policy })
    );
}

#[test]
fn unreachable_required_errors() {
    let p = ClosureProtocol::new(
        Box::new(|| Ok(AttemptClass::Unreachable("net down".into()))),
        3,
    );
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("net down"));
}

#[test]
fn unreachable_best_effort_skips() {
    let p = ClosureProtocol::new(
        Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))),
        3,
    );
    let r = Negotiation::new(p, FailurePolicy::BestEffort).run().unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(s) if s.contains("offline")));
}

#[test]
fn unreachable_gating_stages() {
    let p = ClosureProtocol::new(
        Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))),
        3,
    );
    let r = Negotiation::new(p, FailurePolicy::Gating).run().unwrap();
    assert!(matches!(r, NegotiationResult::Staged(s) if s.contains("offline")));
}

#[test]
fn other_failure_required_errors() {
    let p = ClosureProtocol::new(
        Box::new(|| Ok(AttemptClass::Other("weird".into()))),
        3,
    );
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("weird"));
}

#[test]
fn exhaustion_required_errors() {
    let calls = RefCell::new(0);
    let propose: PropFn = Box::new(|| {
        *calls.borrow_mut() += 1;
        Ok(AttemptClass::Conflict)
    });
    let p = ClosureProtocol::new(propose, 3);
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("gave up"));
    assert_eq!(*calls.borrow(), 3);
}

#[test]
fn exhaustion_best_effort_skips() {
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 2);
    let r = Negotiation::new(p, FailurePolicy::BestEffort).run().unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(s) if s.contains("gave up")));
}

#[test]
fn exhaustion_gating_stages() {
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 2);
    let r = Negotiation::new(p, FailurePolicy::Gating).run().unwrap();
    assert!(matches!(r, NegotiationResult::Staged(s) if s.contains("gave up")));
}

#[test]
fn propagates_propose_error() {
    let p = ClosureProtocol::new(
        Box::new(|| Err(BallError::Other("spawn failed".into()))),
        3,
    );
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("spawn"));
}

#[test]
fn propagates_fetch_error() {
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 3)
        .with_fetch(Box::new(|| Err(BallError::Conflict("unresolvable".into()))));
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("unresolvable"));
}

#[test]
fn propagates_post_merge_error() {
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 3)
        .with_post_merge(Box::new(|| Err(BallError::Other("post-merge boom".into()))));
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("post-merge"));
}

#[test]
fn run_strict_unwraps_ok() {
    let p = ClosureProtocol::new(always_ok(), 3);
    let outcome = Negotiation::new(p, FailurePolicy::Required).run_strict().unwrap();
    assert_eq!(outcome, "pushed");
}

#[test]
fn run_strict_propagates_skipped_as_err() {
    let p = ClosureProtocol::new(
        Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))),
        2,
    );
    let err = Negotiation::new(p, FailurePolicy::BestEffort).run_strict().unwrap_err();
    assert!(format!("{err}").contains("offline"));
}

#[test]
fn run_strict_propagates_staged_as_err() {
    let p = ClosureProtocol::new(Box::new(|| Ok(AttemptClass::Conflict)), 2);
    let err = Negotiation::new(p, FailurePolicy::Gating).run_strict().unwrap_err();
    assert!(format!("{err}").contains("gave up"));
}

/// Trivial Protocol that doesn't override `post_merge` — drives the
/// default-impl branch of the trait method. Returns Conflict for
/// `conflicts_first` calls, then Ok.
struct DefaultPostMergeProtocol {
    conflicts_first: usize,
    calls: usize,
    budget: usize,
}
impl Protocol for DefaultPostMergeProtocol {
    type Outcome = &'static str;
    fn propose(&mut self) -> Result<AttemptClass> {
        self.calls += 1;
        if self.calls <= self.conflicts_first {
            Ok(AttemptClass::Conflict)
        } else {
            Ok(AttemptClass::Ok)
        }
    }
    fn fetch_remote_view(&mut self) -> Result<()> {
        Ok(())
    }
    fn pushed(&mut self) -> Self::Outcome {
        "default-pushed"
    }
    fn retry_budget(&self) -> usize {
        self.budget
    }
}

#[test]
fn protocol_default_post_merge_keeps_loop_retrying_until_ok() {
    // First propose returns Conflict; default post_merge returns None;
    // loop retries; second propose returns Ok -> pushed outcome.
    // Exercises both default post_merge + pushed in one path. The
    // default `commit_policy` (Commit { message: None }) rides along
    // because DefaultPostMergeProtocol does not override it.
    let p = DefaultPostMergeProtocol { conflicts_first: 1, calls: 0, budget: 3 };
    let r = Negotiation::new(p, FailurePolicy::Required).run().unwrap();
    assert_eq!(r, ok_default("default-pushed"));
}
