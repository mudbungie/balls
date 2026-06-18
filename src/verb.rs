//! §9 verbs and their §8 op class.
//!
//! §9 groups verbs three ways and §8 gives the groups two lifecycles: the
//! deliverable verbs author a ball-file diff and seal it ([`OpClass::Mutating`]);
//! read and checkout-lifecycle verbs author no diff, so they have no seal and no
//! change worktree ([`OpClass::Diffless`]).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A balls verb — the user-facing command in `bl <verb>`.
///
/// A verb is also the value of a blocker's `on` ([`crate::task::On`], §10/§15):
/// the op an edge gates IS a verb, so the two are one type. Hence the
/// token-based serde below — a blocker stores `on = "claim"`, not a numeric
/// discriminant — keeping the on-disk form stable and human-legible (§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    // Deliverable lifecycle (§9): mutate a `tasks/<id>.md` file.
    Create,
    Claim,
    Unclaim,
    Update,
    Close,
    /// §16 `bl import` — ingest verbatim, fully-identified task JSON (the
    /// `show --json` bedrock shape) through the real store. The inverse of the
    /// bedrock read: id and timestamps are taken verbatim — no mint, no stamp,
    /// no gate. NOT `create` with an id flag: "reproduce an existing identity"
    /// (migration, restore, federation join) is a distinct primitive from
    /// "mint a new one", which is exactly why `create` refuses foreign ids.
    Import,
    // Reads (§9): author no diff; their hook keys run against the checkouts.
    Show,
    List,
    // Checkout lifecycle (§9/§13): act on the checkout, not a ball.
    Prime,
    Sync,
    Install,
    Conf,
}

/// How §8 runs an op: whether it authors and seals a ball-file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// Authors a ball-file diff in a change worktree and seals it — §8 steps
    /// 1/3/5 are present. The deliverable verbs.
    Mutating,
    /// Authors no ball-file diff: no seal, no change worktree (§8 "skip steps
    /// 1/3/5"). Reads and checkout-lifecycle verbs; their hooks run against
    /// the checkout directly.
    Diffless,
}

impl Verb {
    /// Every verb, in §9 order — the single source the parser and tests draw on.
    pub const ALL: [Verb; 12] = [
        Verb::Create,
        Verb::Claim,
        Verb::Unclaim,
        Verb::Update,
        Verb::Close,
        Verb::Import,
        Verb::Show,
        Verb::List,
        Verb::Prime,
        Verb::Sync,
        Verb::Install,
        Verb::Conf,
    ];

    /// The canonical lower-case token — the inverse of [`Verb::parse`].
    pub fn token(self) -> &'static str {
        match self {
            Verb::Create => "create",
            Verb::Claim => "claim",
            Verb::Unclaim => "unclaim",
            Verb::Update => "update",
            Verb::Close => "close",
            Verb::Import => "import",
            Verb::Show => "show",
            Verb::List => "list",
            Verb::Prime => "prime",
            Verb::Sync => "sync",
            Verb::Install => "install",
            Verb::Conf => "conf",
        }
    }

    /// Resolve a token to its verb, or `None` if unrecognized.
    pub fn parse(token: &str) -> Option<Verb> {
        Verb::ALL.into_iter().find(|v| v.token() == token)
    }

    /// A terse one-line description for the `bl help` command directory (the
    /// "what" alone). The how and why live in the fuller `bl skill` guide; this
    /// is generated from [`Self::ALL`], so the directory can never drift from the
    /// verb set.
    pub fn summary(self) -> &'static str {
        match self {
            Verb::Create => "file a new task; prints its id",
            Verb::Claim => "take a task and materialize its work worktree",
            Verb::Unclaim => "release a claim",
            Verb::Update => "overwrite any field of a task",
            Verb::Close => "deliver the work and archive the task",
            Verb::Import => "bulk-create tasks from JSON on stdin (won't overwrite existing ids — use update to modify)",
            Verb::Show => "show one task in full",
            Verb::List => "list tasks (status, tag, and date filters)",
            Verb::Prime => "ready this checkout (run at session start)",
            Verb::Sync => "pull the store from the remote",
            Verb::Install => "copy committed config/plugins between branches",
            Verb::Conf => "read or write this checkout's local config (with provenance)",
        }
    }

    /// The §8 op class: only the deliverable verbs author and seal a diff.
    pub fn class(self) -> OpClass {
        match self {
            Verb::Create
            | Verb::Claim
            | Verb::Unclaim
            | Verb::Update
            | Verb::Close
            | Verb::Import => OpClass::Mutating,
            Verb::Show
            | Verb::List
            | Verb::Prime
            | Verb::Sync
            | Verb::Install
            | Verb::Conf => OpClass::Diffless,
        }
    }
}

/// Serialize as the canonical lower-case token (§3 on-disk form), so a blocker's
/// `on` reads `"claim"`/`"close"`/`"update"`/… — never an integer.
impl Serialize for Verb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.token())
    }
}

/// Deserialize from the token, rejecting any string that is not a known verb —
/// the inverse of [`Verb::token`], reusing [`Verb::parse`].
impl<'de> Deserialize<'de> for Verb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        Verb::parse(&token).ok_or_else(|| serde::de::Error::custom(format!("unknown op '{token}'")))
    }
}

#[cfg(test)]
#[path = "verb_tests.rs"]
mod tests;
