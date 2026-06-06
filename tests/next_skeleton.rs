//! The greenfield substrate is reachable as public API and runs under the
//! existing test/coverage harness — no separate crate or build wiring (epic
//! bl-72a8: the next major version lives in the same `balls` crate).

use balls::next::op::{Op, Phase};
use balls::next::run;
use balls::next::verb::Verb;

#[test]
fn dispatch_entrypoint_resolves_a_verb_to_its_op_plan() {
    // The §8 spine end to end: argv -> verb -> op -> lifecycle.
    assert_eq!(run(&["prime".to_string()]), 0);

    let op = Op::new(Verb::Prime);
    assert_eq!(op.verb(), Verb::Prime);
    assert_eq!(op.phases(), &[Phase::Pre, Phase::Post]);
}
