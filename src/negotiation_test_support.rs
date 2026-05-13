//! Shared primitive-test harness. A `ClosureProtocol` synthesizes a
//! wire from closures so the loop's behavior is verified independent
//! of any real protocol. Lives in its own module so both
//! `negotiation_tests` and `negotiation_reject_tests` reuse one
//! definition (no duplicated protocol, and `negotiation_tests.rs`
//! stays under the 300-line limit).

use super::*;

pub(crate) type PropFn<'a> = Box<dyn FnMut() -> Result<AttemptClass> + 'a>;
pub(crate) type FetchFn<'a> = Box<dyn FnMut() -> Result<()> + 'a>;
pub(crate) type PostMergeFn<'a> = Box<dyn FnMut() -> Result<Option<&'static str>> + 'a>;

pub(crate) struct ClosureProtocol<'a> {
    propose: PropFn<'a>,
    fetch: FetchFn<'a>,
    post_merge: PostMergeFn<'a>,
    budget: usize,
    policy: CommitPolicy,
}

impl<'a> ClosureProtocol<'a> {
    pub(crate) fn new(propose: PropFn<'a>, budget: usize) -> Self {
        Self {
            propose,
            fetch: Box::new(|| Ok(())),
            post_merge: Box::new(|| Ok(None)),
            budget,
            policy: CommitPolicy::default(),
        }
    }
    pub(crate) fn with_fetch(mut self, f: FetchFn<'a>) -> Self {
        self.fetch = f;
        self
    }
    pub(crate) fn with_post_merge(mut self, f: PostMergeFn<'a>) -> Self {
        self.post_merge = f;
        self
    }
    pub(crate) fn with_policy(mut self, p: CommitPolicy) -> Self {
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

pub(crate) fn always_ok<'a>() -> PropFn<'a> {
    Box::new(|| Ok(AttemptClass::Ok))
}

pub(crate) fn ok_default<O>(outcome: O) -> NegotiationResult<O> {
    NegotiationResult::Ok(Accepted {
        outcome,
        commit_policy: CommitPolicy::default(),
    })
}
