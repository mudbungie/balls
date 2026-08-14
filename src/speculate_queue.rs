//! The merging queue — order as a query over git tags (bl-5c5f, design
//! docs/design/bl-24e7-speculative-merge-queue.md).
//!
//! A ball enters the merge queue by planting an annotated tag
//! `merging/<id>` on the tip of its `work/<id>` branch. That one ref is the
//! whole mechanism, three facts in one object, none stored anywhere else:
//!
//! - **membership** — the tag exists;
//! - **position** — the taggerdate orders the queue (refname breaks
//!   same-second ties, deterministically);
//! - **the seal** — the tag's target names the exact commit sealed. A branch
//!   that moves past its tag is UNSEALED by derivation: fixing a gate failure
//!   means committing, committing moves the tip off the tag, and re-entering
//!   means re-tagging — which is a new date, the bottom of the queue. Eviction
//!   has no mechanism of its own.
//!
//! Reads never mutate ([`queue`] reports unsealed entries rather than reaping
//! them — sweeping is the speculator's job, bl-d0c2, backstopped by prime's
//! debris pass), and nothing here touches a remote: the queue is a
//! single-substrate coordination surface, like every other derived status in
//! balls (§3 — status is computed on read, absence is the record).
//!
//! Env-free like [`crate::speculate`]: the repo path and the optional
//! timestamp arrive as arguments; `bl-speculate` is the env-reading edge.

use std::io;
use std::path::Path;

use crate::safegit;

/// One queue entry, as derived from a `merging/<id>` tag.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// The ball id — the tag name with `merging/` stripped.
    pub id: String,
    /// The sealed commit — the tag's peeled target.
    pub tip: String,
    /// TRUE iff `work/<id>` still points at the sealed commit. An unsealed
    /// entry holds no position: its branch moved (or vanished — a landed
    /// close deletes it), so it is out of the queue in every consumer's eyes.
    pub sealed: bool,
}

/// Seal `work/<id>` into the queue: plant `merging/<id>` on its tip, replacing
/// any earlier tag — so re-enqueue IS requeue-at-bottom (a fresh taggerdate),
/// the invariant the design leans on. `date`, when given, becomes the
/// taggerdate (tests order deterministically with it; the edge passes None).
/// The tagger identity is fixed and mechanical — the tag is a seal marker,
/// not authorship; builder identity lives in the verdict records.
pub fn enqueue(repo: &Path, id: &str, date: Option<&str>) -> io::Result<String> {
    let mut resolve = safegit::at(repo);
    resolve.args(["rev-parse", "--verify"]).arg(format!("refs/heads/work/{id}"));
    let tip = ok_stdout(&resolve.output()?, "git rev-parse work branch")?;
    seal(repo, id, &tip, date)?;
    Ok(tip)
}

/// Plant `merging/<id>` on `target` — the tag-write half of [`enqueue`],
/// shared with [`adopt`], which seals a LISTED sha rather than re-resolving
/// the branch (so a branch a concurrent close deletes mid-pass yields a tag
/// that derives unsealed and is swept next pass, not an error).
fn seal(repo: &Path, id: &str, target: &str, date: Option<&str>) -> io::Result<()> {
    let mut tag = safegit::at(repo);
    tag.args(["-c", "user.name=bl-speculate", "-c", "user.email=speculate@balls"]);
    tag.args(["tag", "--force", "-a", "-m", "sealed for the merge queue (bl-24e7)"]);
    tag.arg(format!("merging/{id}")).arg(target);
    if let Some(d) = date {
        tag.env("GIT_COMMITTER_DATE", d);
    }
    ok_stdout(&tag.output()?, "git tag").map(|_| ())
}

/// Adopt every quiet `work/<id>` tip into the queue (bl-b761): seal each
/// branch not already sealed AT its tip, in for-each-ref (refname) order, and
/// return `(id, tip)` per seal planted. The paved path: an agent that only
/// ever commits and closes still rides the queue, because the speculator
/// seals on its behalf — nobody needs to know which commit was the last one,
/// the last one's seal is simply the one that survives. Idempotent by the
/// skip: a tip already sealed is not re-sealed, so standing entries keep
/// their position. Called at pass END ([`crate::speculate_run`]) so a fresh
/// seal must survive one full inter-pass interval before it is built —
/// quiescence measured in passes, no clock consulted.
pub fn adopt(repo: &Path, date: Option<&str>) -> io::Result<Vec<(String, String)>> {
    let sealed: Vec<(String, String)> =
        queue(repo)?.into_iter().filter(|e| e.sealed).map(|e| (e.id, e.tip)).collect();
    let mut cmd = safegit::at(repo);
    cmd.args(["for-each-ref", "--format=%(refname:short) %(objectname)", "refs/heads/work/"]);
    let listing = ok_stdout(&cmd.output()?, "git for-each-ref work branches")?;
    let mut adopted = Vec::new();
    for line in listing.lines() {
        let (id, tip) = work_tip(line)?;
        if sealed.iter().any(|(sid, stip)| sid == id && stip == tip) {
            continue;
        }
        seal(repo, id, tip, date)?;
        adopted.push((id.to_string(), tip.to_string()));
    }
    Ok(adopted)
}

/// Parse one `for-each-ref` line from [`adopt`]'s listing into `(id, tip)`.
fn work_tip(line: &str) -> io::Result<(&str, &str)> {
    let (name, tip) = line
        .split_once(' ')
        .ok_or_else(|| io::Error::other(format!("unparseable for-each-ref line: {line:?}")))?;
    Ok((name.strip_prefix("work/").unwrap_or(name), tip))
}

/// Leave the queue: delete `merging/<id>`. Landing, abandoning, and eviction
/// cleanup are all this one act — the tag's absence is the record.
pub fn dequeue(repo: &Path, id: &str) -> io::Result<()> {
    let mut cmd = safegit::at(repo);
    cmd.args(["tag", "-d"]).arg(format!("merging/{id}"));
    ok_stdout(&cmd.output()?, "git tag -d").map(|_| ())
}

/// The queue, in position order: every `merging/*` tag sorted by taggerdate
/// (refname on ties), each checked against its `work/<id>` tip for the seal.
/// Unsealed entries are still REPORTED — a consumer deciding what to sweep
/// needs to see them — but hold no position a speculator may build on.
pub fn queue(repo: &Path) -> io::Result<Vec<Entry>> {
    let mut cmd = safegit::at(repo);
    cmd.args(["for-each-ref", "--sort=refname", "--sort=taggerdate"]);
    cmd.args(["--format=%(refname:short) %(*objectname)", "refs/tags/merging/"]);
    let listing = ok_stdout(&cmd.output()?, "git for-each-ref")?;
    listing.lines().map(|line| entry(repo, line)).collect()
}

/// Parse one for-each-ref line and derive the seal from the live branch tip.
fn entry(repo: &Path, line: &str) -> io::Result<Entry> {
    let (name, tip) = line
        .split_once(' ')
        .ok_or_else(|| io::Error::other(format!("unparseable for-each-ref line: {line:?}")))?;
    let id = name.strip_prefix("merging/").unwrap_or(name).to_string();
    let mut resolve = safegit::at(repo);
    resolve.args(["rev-parse", "--verify", "--quiet"]).arg(format!("refs/heads/work/{id}"));
    let branch = resolve.output()?;
    let sealed = branch.status.success()
        && String::from_utf8_lossy(&branch.stdout).trim() == tip;
    Ok(Entry { id, tip: tip.to_string(), sealed })
}

/// Success → trimmed stdout; failure → an error carrying git's stderr voice.
fn ok_stdout(out: &std::process::Output, what: &str) -> io::Result<String> {
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let voice = String::from_utf8_lossy(&out.stderr);
        Err(io::Error::other(format!("{what}: {}", voice.trim())))
    }
}

#[cfg(test)]
#[path = "speculate_queue_tests.rs"]
mod speculate_queue_tests;
