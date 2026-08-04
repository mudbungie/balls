//! The delivered-but-unsealed close voice (bl-739b), on throwaway project
//! repos: a close whose squash already stands is re-voiced with the commit and
//! the ref it is on; every other abort — and every other verb — passes through
//! untouched, because the note is DERIVED from the project repo, never inferred
//! from the fact that a close failed.

use super::*;
use crate::delivery_repo::tests::project;
use std::fs;
use std::path::PathBuf;

/// The generic §8 seal refusal ([`crate::git`]) — the error this amends.
fn seal_lost() -> io::Error {
    io::Error::other(
        "sealing onto the anvil failed: the store moved under this op — a concurrent `bl` won \
         the seal; nothing was written",
    )
}

/// Deliver `id`'s one-commit work branch onto `integration`, returning the
/// project root and the tempdir (kept alive).
fn delivered(id: &str, integration: &str) -> (tempfile::TempDir, PathBuf) {
    let (tmp, root, p) = project();
    let branch = work_branch(id);
    let wt = tmp.path().join(id);
    p.materialize(&wt, &branch).unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    let subject = format!("Add feature [{id}]");
    p.deliver(&wt, &branch, integration, &subject, &marker(id)).unwrap();
    (tmp, root)
}

#[test]
fn a_close_whose_squash_landed_is_re_voiced_with_the_commit_and_the_ref() {
    let (_tmp, root) = delivered("bl-x", "main");
    let sha = Project::run(&root, &["rev-parse", "main"]).unwrap().trim().to_string();

    let out = amend(seal_lost(), &root, Verb::Close, "bl-x", None).to_string();
    // The original abort SURVIVES — the note corrects its scope, never replaces it.
    assert!(out.contains("nothing was written"), "{out}");
    assert!(out.contains("delivered, not sealed"), "{out}");
    assert!(out.contains(&format!("commit {sha} is on main.")), "{out}");
    assert!(out.contains("Do not redo the work and do not unclaim"), "{out}");
    assert!(out.contains("Re-run `bl close`"), "{out}");
}

#[test]
fn every_other_verb_passes_through_even_when_a_delivery_stands() {
    // The reason this seam is close-only: on a `create` (or any op the delivery
    // plugin is not wired to) "your code is already on main" would be a lie,
    // even though the very same tag sits on the very same branch.
    let (_tmp, root) = delivered("bl-x", "main");
    let out = amend(seal_lost(), &root, Verb::Create, "bl-x", None).to_string();
    assert_eq!(out, seal_lost().to_string());
}

#[test]
fn a_close_that_aborted_before_its_squash_passes_through() {
    // The gate failure / stale source / rejected delivery CAS: the work branch
    // exists, no `[bl-x]` delivery stands since it forked — silence.
    let (tmp, _root, p) = project();
    p.materialize(&tmp.path().join("wt"), "work/bl-x").unwrap();

    let out = amend(seal_lost(), &p.root, Verb::Close, "bl-x", None).to_string();
    assert_eq!(out, seal_lost().to_string());
}

#[test]
fn an_unprovable_delivery_is_silence_not_a_second_abort() {
    // Two git failures, both on an already-failing path. A ball with no
    // `work/<id>` branch at all (the merge-base errors), and an invocation path
    // that is not a git repo (the integration read errors) — neither may turn
    // one abort into another.
    let (tmp, root, _p) = project();
    assert_eq!(amend(seal_lost(), &root, Verb::Close, "bl-none", None).to_string(), seal_lost().to_string());

    let bare = tmp.path().join("not-a-repo");
    fs::create_dir(&bare).unwrap();
    assert_eq!(amend(seal_lost(), &bare, Verb::Close, "bl-x", None).to_string(), seal_lost().to_string());
}

#[test]
fn a_nested_close_names_the_epic_ref_it_delivered_into() {
    // bl-7b71: the target is `work/<epic>`, not the integration branch, and the
    // note must send the operator to where the code actually is. The ref is
    // READ from the wire-derived target, never minted.
    let (tmp, root, p) = project();
    p.mint("work/bl-epic", "main").unwrap();
    let wt = tmp.path().join("kid");
    p.materialize(&wt, "work/bl-kid").unwrap();
    fs::write(wt.join("kid.txt"), "kid\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "kid work"]).unwrap();
    p.deliver(&wt, "work/bl-kid", "work/bl-epic", "Kid [bl-kid]", "[bl-kid]").unwrap();
    let sha = Project::run(&root, &["rev-parse", "work/bl-epic"]).unwrap().trim().to_string();

    let out = amend(seal_lost(), &root, Verb::Close, "bl-kid", Some("bl-epic")).to_string();
    assert!(out.contains(&format!("[bl-kid] delivery commit {sha} is on work/bl-epic.")), "{out}");
    // main never moved: the note points at the epic ref, which is the truth.
    assert!(!out.contains("is on main"), "{out}");
}
