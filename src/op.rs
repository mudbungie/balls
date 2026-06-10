//! §8 op lifecycle — the named phases of the verb-agnostic shape.
//!
//! balls authors a base change, an ordered plugin chain acts on it, balls
//! SEALS it (commit + integrate, atomically), and plugins react. The seal is
//! the pre/post boundary: `pre` modifiers shape what gets sealed, `post`
//! reactors act on the now-landed record. [`Phase`] names the steps;
//! [`crate::lifecycle::Engine`] runs them — the full five for a mutating op
//! ([`Engine::seal`](crate::lifecycle::Engine::seal)), `pre`/`post` alone for
//! a diffless one (§8 "skip steps 1/3/5") — and unwinds them in reverse on any
//! abort (§14).

/// One step of the §8 lifecycle. A mutating op runs all five in order; a
/// diffless op runs only [`Phase::Pre`] and [`Phase::Post`] — it authors and
/// seals nothing (§8 "skip steps 1/3/5").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// (1) balls makes the change worktree and stages the op's base diff.
    Author,
    /// (2) pre modifiers run in hook-list order; they shape what gets sealed.
    Pre,
    /// (3) balls SEALS — commit + integrate onto the anvil, atomically.
    Seal,
    /// (4) post reactors run in hook-list order; they act on the sealed record.
    Post,
    /// (5) teardown — balls removes the change worktree.
    Teardown,
}

impl Phase {
    /// The short §6 label — the `<phase>` argv token a plugin is invoked (and
    /// rolled back) with, and the phase tag on its log envelope
    /// ([`crate::plugin`]).
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
    fn every_phase_has_its_canonical_token() {
        let all = [Phase::Author, Phase::Pre, Phase::Seal, Phase::Post, Phase::Teardown];
        let tokens: Vec<&str> = all.iter().map(|p| p.token()).collect();
        assert_eq!(tokens, ["author", "pre", "seal", "post", "teardown"]);
    }
}
