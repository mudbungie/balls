//! SPEC §8.1 / §17.19 — `Reject` is a first-class veto: distinct
//! from `Conflict` (no merge, no retry) and `Other` (a decision, not
//! a wire fault), routed by the failure policy and carrying the
//! plugin's reason verbatim. Reuses the shared `ClosureProtocol` so
//! the harness's own lines stay covered by `negotiation_tests`.

use super::test_support::*;
use super::*;
use std::cell::RefCell;

fn reject_each(calls: &RefCell<usize>) -> PropFn<'_> {
    Box::new(|| {
        *calls.borrow_mut() += 1;
        Ok(AttemptClass::Reject("ci is red".into()))
    })
}

#[test]
fn reject_required_aborts_with_reason_and_no_retry() {
    let calls = RefCell::new(0);
    let p = ClosureProtocol::new(reject_each(&calls), 5);
    let err = Negotiation::new(p, FailurePolicy::Required).run().unwrap_err();
    assert!(format!("{err}").contains("ci is red"));
    assert_eq!(*calls.borrow(), 1, "a veto must not consume the retry budget");
}

#[test]
fn reject_best_effort_skips_with_reason() {
    let calls = RefCell::new(0);
    let p = ClosureProtocol::new(reject_each(&calls), 5);
    let r = Negotiation::new(p, FailurePolicy::BestEffort).run().unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(s) if s.contains("ci is red")));
}

#[test]
fn reject_gating_stages_with_reason() {
    let calls = RefCell::new(0);
    let p = ClosureProtocol::new(reject_each(&calls), 5);
    let r = Negotiation::new(p, FailurePolicy::Gating).run().unwrap();
    assert!(matches!(r, NegotiationResult::Staged(s) if s.contains("ci is red")));
}
