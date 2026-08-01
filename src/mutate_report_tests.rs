//! §9 reporting tests — the two things this layer must never do: re-derive an
//! id the op already holds, and fail an op whose seal is already durable
//! (bl-dede).

use super::*;
use tempfile::tempdir;

/// A throwaway checkout carrying one commit whose message is `msg`; returns its
/// sha. Plumbing only — no worktree round-trip needed to read `%B` back.
fn commit_with(dir: &Path, msg: &str) -> String {
    git::run(dir, &["init", "-q"], None).unwrap();
    git::run(dir, &["config", "user.name", "t"], None).unwrap();
    git::run(dir, &["config", "user.email", "t@example.com"], None).unwrap();
    let tree = git::run(dir, &["mktree"], Some("")).unwrap().trim().to_string();
    git::run(dir, &["commit-tree", &tree], Some(msg)).unwrap().trim().to_string()
}

#[test]
fn a_non_create_report_never_touches_the_store_or_the_sealed_commit() {
    // The op NAMED this ball before it sealed, so the report has nothing to look
    // up: neither the store path nor the sha below exists, and `claim` still
    // reports. This is the bl-dede pin — the re-read it used to do here ran a
    // subprocess on the far side of the plugin chain, after `close.post` had
    // deleted the caller's cwd, and turned a landed close into exit 1.
    let nowhere = Path::new("/nonexistent-store-bl-dede");
    emit(Verb::Claim, nowhere, "bl-1", "not-a-sha").unwrap();
    emit(Verb::Unclaim, nowhere, "bl-1", "not-a-sha").unwrap();
    emit(Verb::Update, nowhere, "bl-1", "not-a-sha").unwrap();
}

#[test]
fn create_reads_its_minted_id_back_from_the_sealed_commit() {
    // `create` alone re-derives: a `create/pre` plugin may have reassigned the
    // id, so the commit — not the pre-seal mint — is authoritative (§5).
    let d = tempdir().unwrap();
    let sha = commit_with(d.path(), "A task\n\nbl-protocol: 1\nbl-op: create\nbl-id: bl-xyz\n");
    minted(d.path(), &sha).unwrap();
}

#[test]
fn create_warns_instead_of_failing_when_the_trailer_cannot_be_confirmed() {
    // The ball exists the moment the commit is on the branch, so an unreadable
    // trailer is a lost id, not a lost ball: warn, exit 0. Erroring would tell
    // `id=$(bl create …)` its ball was never created (bl-dede).
    let d = tempdir().unwrap();
    let sha = commit_with(d.path(), "A task with no trailer block at all\n");
    minted(d.path(), &sha).unwrap();
}

#[test]
fn a_commit_the_store_cannot_produce_is_still_an_error() {
    // The one genuine corruption: `git log` itself cannot answer for the sha the
    // seal just returned. That is not a reporting hiccup, so it stays an error.
    let d = tempdir().unwrap();
    commit_with(d.path(), "A task\n");
    assert!(minted(d.path(), "0000000000000000000000000000000000000000").is_err());
}
