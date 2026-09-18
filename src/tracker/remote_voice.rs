//! The reconcile's two spoken outcomes (bl-21ab), lifted from
//! [`super`] (`remote_ops`) at the 300-line cap: the same-ball refusal in
//! balls' voice, and the fail-open transport warning.

use super::git;
use std::io;
use std::path::Path;

/// A SAME-BALL CONFLICT, in balls' voice (bl-3129's precedent, one layer out at
/// the remote) — E5 sharpened to name the ball (bl-21ab). The rebase stopped
/// on a `tasks/<id>.md` both sides changed and was aborted, so the two facts
/// are: nothing was published, and nothing local was changed. The exits are
/// the operator's, stated as a choice: for an OP, core's abort un-seals it —
/// `bl sync` then re-run (a claim of a ball someone else already claimed is
/// contention, and the later claim loses; a close over a remote update is a
/// human's call); for `sync`, the seals stay local and the operator resolves
/// them in the store checkout, after which `bl sync` publishes — or discards
/// them to the remote's version. balls never merges a ball field-wise. Git's
/// own rebase text is NOT appended: the conflict is positively identified (the
/// unmerged index named the ball), so its seven `hint:` lines carry nothing
/// (bl-ce2d — the bl-3129 rule, raw git only for non-contention failures).
pub(super) fn conflict(store: &str, remote: &str, branch: &str, contended: &[String]) -> io::Error {
    let who = if contended.is_empty() { "a ball".to_string() } else { contended.join(", ") };
    io::Error::other(format!(
        "push rejected: `{remote}`'s `{branch}` moved and {who} changed on both sides — the rebase of this \
         store's unpublished seals was aborted; nothing was published and nothing local was changed. If \
         this was an op it has un-sealed: run `bl sync`, then re-run it (a ball someone else already \
         claimed is contention — the later claim loses). If this store still holds seals of its own, \
         reconcile them in the store checkout, {store}: `git log FETCH_HEAD..{branch}` lists them; `git \
         rebase FETCH_HEAD`, resolve the named file, then `bl sync` publishes — or `git reset --hard \
         FETCH_HEAD` to take the remote's version"
    ))
}

/// The ball ids whose files are UNMERGED in a stopped rebase (`git ls-files
/// -u`: one line per stage, `<mode> <sha> <stage>\t<path>`) — a `tasks/<id>.md`
/// reads as its id, anything else as its path. Modify/modify and modify/delete
/// (a close against an update) both list here.
pub(super) fn unmerged_balls(store: &Path) -> Vec<String> {
    let mut ids: Vec<String> = git(store, &["ls-files", "-u"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .map(|p| p.strip_prefix("tasks/").and_then(|f| f.strip_suffix(".md")).unwrap_or(p).to_string())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Transport failure FAILS OPEN (bl-3616 §3, the line github-issues drew in
/// bl-a95c, moved into the tracker): warn on stderr and return `Ok` — the store
/// stays ahead of a remote it cannot reach, nothing is lost, and the drift
/// render (`bl show`/`bl list`) keeps saying so until a reachable op or `sync`
/// publishes. The op is NOT aborted: an unreachable hub must not stop local
/// work, and a rejected op would un-seal work that is perfectly good.
pub(super) fn fail_open(remote: &str, e: &io::Error) {
    // Git's FIRST line names the cause; the rest is boilerplate advice.
    let cause = e.to_string().lines().next().unwrap_or_default().to_string();
    eprintln!("tracker: `{remote}` is unreachable — this store stays ahead of it, unpublished; the next op or `bl sync` publishes once it is reachable ({cause})");
}

