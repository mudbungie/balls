//! `bl --skill` / `bl skill` (the top-level operating guide) and `bl <cmd>
//! --skill` (one command's full usage, into which the per-command `--help` is
//! folded). The agent documentation, embedded so it works from a bare `cargo
//! install` with no repo checkout to read from.
//!
//! Like `help`, `skill` is help OUTPUT, not an op — it authors no diff, has no
//! lifecycle, and is never a blocker target — so it is dispatched directly in
//! [`crate::run`] and kept OUT of the [`Verb`] enum (which doubles as a blocker's
//! `on`, §10). The per-command [`command`] match is exhaustive over [`Verb`] (no
//! `_` arm), so a new verb cannot ship without authoring its `skill/<verb>.md` —
//! the same single-source discipline the generated `bl help` directory keeps for
//! the one-line summaries, one rung deeper.

use crate::verb::Verb;

/// The top-level operating guide — `bl --skill` (canonical) and `bl skill`
/// (deprecated). Architecture, the footgun invariants, and the command map; the
/// per-command depth is one level down under [`command`]. Embedded from the
/// repo-root `SKILL.md`.
pub fn top() -> &'static str {
    include_str!("../SKILL.md")
}

/// One command's full usage — `bl <cmd> --skill`, and its folded `--help` / `-h`
/// / `bl help <cmd>` aliases (and the footer [`crate::run`] prints on a usage
/// error). Exhaustive over [`Verb`]: a new verb must bring its `skill/<verb>.md`.
pub fn command(verb: Verb) -> &'static str {
    match verb {
        Verb::Create => include_str!("../skill/create.md"),
        Verb::Claim => include_str!("../skill/claim.md"),
        Verb::Unclaim => include_str!("../skill/unclaim.md"),
        Verb::Update => include_str!("../skill/update.md"),
        Verb::Close => include_str!("../skill/close.md"),
        Verb::Import => include_str!("../skill/import.md"),
        Verb::Show => include_str!("../skill/show.md"),
        Verb::List => include_str!("../skill/list.md"),
        Verb::Prime => include_str!("../skill/prime.md"),
        Verb::Sync => include_str!("../skill/sync.md"),
        Verb::Install => include_str!("../skill/install.md"),
        Verb::Conf => include_str!("../skill/conf.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_guide_is_the_embedded_operating_guide() {
        let g = top();
        assert!(g.contains("balls"), "the top guide is non-empty");
        // It sends the reader down to the per-command depth.
        assert!(g.contains("--skill"), "the top guide points at per-command --skill");
    }

    #[test]
    fn every_verb_has_a_skill_doc_naming_itself() {
        // Exhaustive over Verb::ALL — covers every match arm, and enforces that a
        // new verb ships with its doc. Each doc names its own verb and leads with
        // a `usage: bl <verb>` line.
        for v in Verb::ALL {
            let doc = command(v);
            assert!(doc.contains(v.token()), "{}'s doc names the verb", v.token());
            assert!(doc.contains("usage: bl "), "{}'s doc has a usage line", v.token());
        }
    }
}
