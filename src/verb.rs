//! §9 verbs and their §8 op class.
//!
//! §9 groups verbs three ways and §8 gives the groups two lifecycles: the
//! deliverable verbs author a ball-file diff and seal it ([`OpClass::Mutating`]);
//! read and checkout-lifecycle verbs author no diff, so they have no seal and no
//! change worktree ([`OpClass::Diffless`]).

/// A balls verb — the user-facing command in `bl <verb>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    // Deliverable lifecycle (§9): mutate a `tasks/<id>.md` file.
    Create,
    Claim,
    Unclaim,
    Update,
    Close,
    Drop,
    // Reads (§9): author no diff; hook dirs only.
    Show,
    List,
    Ready,
    DepTree,
    // Checkout lifecycle (§9/§13): act on the checkout, not a ball.
    Prime,
    Sync,
    Install,
}

/// How §8 runs an op: whether it authors and seals a ball-file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// Authors a ball-file diff in a change worktree and seals it — §8 steps
    /// 1/3/5 are present. The deliverable verbs.
    Mutating,
    /// Authors no ball-file diff: no seal, no change worktree (§8 "skip steps
    /// 1/3/5"). Reads and checkout-lifecycle verbs; their hooks run against
    /// `operating/` directly.
    Diffless,
}

impl Verb {
    /// Every verb, in §9 order — the single source the parser and tests draw on.
    pub const ALL: [Verb; 13] = [
        Verb::Create,
        Verb::Claim,
        Verb::Unclaim,
        Verb::Update,
        Verb::Close,
        Verb::Drop,
        Verb::Show,
        Verb::List,
        Verb::Ready,
        Verb::DepTree,
        Verb::Prime,
        Verb::Sync,
        Verb::Install,
    ];

    /// The canonical lower-case token — the inverse of [`Verb::parse`].
    pub fn token(self) -> &'static str {
        match self {
            Verb::Create => "create",
            Verb::Claim => "claim",
            Verb::Unclaim => "unclaim",
            Verb::Update => "update",
            Verb::Close => "close",
            Verb::Drop => "drop",
            Verb::Show => "show",
            Verb::List => "list",
            Verb::Ready => "ready",
            Verb::DepTree => "dep-tree",
            Verb::Prime => "prime",
            Verb::Sync => "sync",
            Verb::Install => "install",
        }
    }

    /// Resolve a token to its verb, or `None` if unrecognized.
    pub fn parse(token: &str) -> Option<Verb> {
        Verb::ALL.into_iter().find(|v| v.token() == token)
    }

    /// The §8 op class: only the deliverable verbs author and seal a diff.
    pub fn class(self) -> OpClass {
        match self {
            Verb::Create
            | Verb::Claim
            | Verb::Unclaim
            | Verb::Update
            | Verb::Close
            | Verb::Drop => OpClass::Mutating,
            Verb::Show
            | Verb::List
            | Verb::Ready
            | Verb::DepTree
            | Verb::Prime
            | Verb::Sync
            | Verb::Install => OpClass::Diffless,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_round_trips_through_its_token() {
        for v in Verb::ALL {
            assert_eq!(Verb::parse(v.token()), Some(v));
        }
    }

    #[test]
    fn unknown_token_does_not_parse() {
        assert_eq!(Verb::parse("frobnicate"), None);
    }

    #[test]
    fn only_deliverable_verbs_are_mutating() {
        let mutating = [
            Verb::Create,
            Verb::Claim,
            Verb::Unclaim,
            Verb::Update,
            Verb::Close,
            Verb::Drop,
        ];
        for v in Verb::ALL {
            let expected = if mutating.contains(&v) {
                OpClass::Mutating
            } else {
                OpClass::Diffless
            };
            assert_eq!(v.class(), expected);
        }
    }
}
