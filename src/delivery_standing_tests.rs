//! The already-delivered guard (bl-c231): content-containment, not
//! commit-presence, decides a retried close — contained skips (the bl-430e
//! retry AND the forge squash-merge), content beyond the delivery aborts
//! loudly (the bl-65e0 handoff), instead of being silently stranded.

use std::fs;

use crate::delivery::Repo;
use crate::delivery_repo::tests::{project, tip};
use crate::delivery_repo::Project;

#[test]
fn deliver_aborts_when_the_branch_carries_content_beyond_its_delivery() {
    // The bl-65e0 handoff onto a delivered-but-unsealed close: A's close
    // squashed onto main but the op aborted before the seal/push; B reclaims
    // the surviving branch and commits MORE work. The old tag-presence skip
    // silently stranded B's commit — the guard must abort loudly instead.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "Add feature [bl-x]"); // A's squash stands
    // B's post-delivery work on the surviving branch.
    fs::write(wt.join("more.txt"), "undelivered\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "more work"]).unwrap();

    let err = p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("already delivered"), "{err}");
    assert!(err.to_string().contains("undelivered changes"), "{err}");
    // Nothing was minted or stranded: main untouched, the branch keeps the work.
    assert_eq!(tip(&root), "Add feature [bl-x]");
    assert!(Project::ok(&root, &["cat-file", "-e", "work/bl-x:more.txt"]).unwrap());
}

#[test]
fn deliver_skips_a_forge_squash_merge_whose_content_landed() {
    // The forge flow (bl-7bfe): the PR is squash-merged ON THE FORGE, so the
    // delivery commit shares no ancestry with work/<id>'s commits — yet their
    // content landed verbatim. Containment, not ancestry, must call this a
    // skip, or every forge close would false-abort.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    // The forge's squash-merge: the same content lands on main as a NEW commit
    // (no shared ancestry with the wip commit), [bl-id] in the subject.
    fs::write(root.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "Add feature (#7) [bl-x]"]).unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(p.marked("main", "[bl-x]").unwrap().len(), 1); // no duplicate squash
    assert_eq!(tip(&root), "Add feature (#7) [bl-x]");
}

#[test]
fn prune_preserves_a_diverged_branch_carrying_work_beyond_its_delivery() {
    // The same divergence prune-side: a delivered branch with content beyond
    // its delivery is NOT settled — deleting it would lose the bl-65e0 work
    // the close's guard just refused to strand.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    fs::write(wt.join("more.txt"), "undelivered\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "more work"]).unwrap();
    p.release(&wt).unwrap(); // teardown: the branch is the only copy

    p.prune().unwrap();
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-x"]).unwrap());
}
