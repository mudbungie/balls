//! Claim-age — the derived "how long has this been held" fact (bl-46ef), read
//! at RENDER from the store's git history and NEVER stored (§3 derived-fact
//! rule). It follows the §11 worktree-line / bl-0e16 journal precedent exactly:
//! a derived fact paints the HUMAN render alone, bedrock `--json` stays the
//! stored-frontmatter mirror and never pays the walk.
//!
//! Claim time already exists in the store — no new field. It is the timestamp
//! of the NEWEST commit touching `tasks/<id>.md` whose §5 trailer is
//! `bl-op: claim`; one `git log -1 --grep` per claimed row derives it.
//! Newest-wins resolves an unclaim/reclaim cycle to the CURRENT claim, the same
//! recency discipline as every §9 history read. The claimed set is fleet-sized,
//! not store-sized, so the walk stays cheap and is only paid for a row that
//! actually renders an age (a live, currently-claimed one).

use std::io;
use std::path::Path;

use crate::git;

/// The unix second of the CURRENT claim on `tasks/<id>.md`: the newest store
/// commit whose §5 `bl-op` trailer is `claim`. `None` when the file carries no
/// claim commit — never claimed, or a `claimant` hand-set on the frontmatter
/// (import) with no claim op behind it — so the caller renders the bare
/// `@claimant` rather than invent an age. Newest-first `git log -1` picks the
/// live claim across an unclaim/reclaim cycle (§9 recency).
pub(crate) fn claimed_at(store: &Path, id: &str) -> io::Result<Option<i64>> {
    let path = format!("tasks/{id}.md");
    let log = git::run(store, &["log", "-1", "--format=%ct", "--grep=^bl-op: claim$", "--", &path], None)?;
    let at = log.trim().parse().ok();
    Ok(at)
}

/// Humanize an age in seconds to ONE coarse unit — minutes under the hour,
/// hours under the day, days beyond. A negative age (host clock skew, a claim
/// stamped in the future) floors at `0m` rather than render a `-` sign.
pub(crate) fn humanize(age_secs: i64) -> String {
    let s = age_secs.max(0);
    if s < 3_600 {
        format!("{}m", s / 60)
    } else if s < 86_400 {
        format!("{}h", s / 3_600)
    } else {
        format!("{}d", s / 86_400)
    }
}

#[cfg(test)]
#[path = "claim_age_tests.rs"]
mod tests;
