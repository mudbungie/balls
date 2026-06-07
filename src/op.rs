//! §8 op lifecycle — the verb-agnostic shape every op runs.
//!
//! balls authors a base change, an ordered plugin chain acts on it, balls
//! SEALS it (commit + integrate, atomically), and plugins react. The seal is
//! the pre/post boundary: `pre` modifiers shape what gets sealed, `post`
//! reactors act on the now-landed record. This skeleton names the phases and
//! the per-class sequence; the work inside each phase is the seam each rewrite
//! phase fills in.

use crate::verb::{OpClass, Verb};

/// One step of the §8 lifecycle. A mutating op runs all five in order; a
/// diffless op runs only [`Phase::Pre`] and [`Phase::Post`] — it authors and
/// seals nothing (§8 "skip steps 1/3/5").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// (1) balls makes the change worktree and stages the op's base diff.
    Author,
    /// (2) pre modifiers run in `NN-` order; they shape what gets sealed.
    Pre,
    /// (3) balls SEALS — commit + integrate onto the anvil, atomically.
    Seal,
    /// (4) post reactors run in `NN-` order; they act on the sealed record.
    Post,
    /// (5) teardown — balls removes the change worktree.
    Teardown,
}

const MUTATING_PHASES: [Phase; 5] = [
    Phase::Author,
    Phase::Pre,
    Phase::Seal,
    Phase::Post,
    Phase::Teardown,
];

const DIFFLESS_PHASES: [Phase; 2] = [Phase::Pre, Phase::Post];

/// A resolved op: a verb plus the §8 lifecycle it will run. No phase does any
/// work yet — that is the seam each rewrite phase fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    verb: Verb,
}

impl Op {
    /// Resolve a verb to its op.
    pub fn new(verb: Verb) -> Op {
        Op { verb }
    }

    /// The verb this op delivers.
    pub fn verb(self) -> Verb {
        self.verb
    }

    /// The ordered §8 phases this op runs, determined by the verb's class.
    pub fn phases(self) -> &'static [Phase] {
        match self.verb.class() {
            OpClass::Mutating => &MUTATING_PHASES,
            OpClass::Diffless => &DIFFLESS_PHASES,
        }
    }

    /// A one-line description of the op and the lifecycle it will run — the
    /// skeleton dispatch's whole output until the phases gain behavior.
    pub fn plan(self) -> String {
        let steps: Vec<&str> = self.phases().iter().map(|p| p.token()).collect();
        format!("{}: {}", self.verb.token(), steps.join(" -> "))
    }
}

impl Phase {
    /// A short label for [`Op::plan`].
    pub fn token(self) -> &'static str {
        match self {
            Phase::Author => "author",
            Phase::Pre => "pre",
            Phase::Seal => "seal",
            Phase::Post => "post",
            Phase::Teardown => "teardown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_remembers_its_verb() {
        assert_eq!(Op::new(Verb::Claim).verb(), Verb::Claim);
    }

    #[test]
    fn a_mutating_op_runs_all_five_phases_in_order() {
        let op = Op::new(Verb::Close);
        assert_eq!(op.phases(), &MUTATING_PHASES);
        assert_eq!(op.plan(), "close: author -> pre -> seal -> post -> teardown");
    }

    #[test]
    fn a_diffless_op_runs_only_pre_and_post() {
        let op = Op::new(Verb::Show);
        assert_eq!(op.phases(), &DIFFLESS_PHASES);
        assert_eq!(op.plan(), "show: pre -> post");
    }
}
