//! Stale-read close guard (bl-9f1d) — a seen-token CAS on task content.
//!
//! The race: an agent claims a ball, another agent updates the task file
//! mid-flight, and the claimant closes without ever seeing the update — sealing
//! an amended contract blind. Same invariant class as the delivery update-ref
//! CAS (bl-a3bb) one layer up: close must act on the content it believes it
//! acts on.
//!
//! The invariant: `bl close <id>` refuses iff the task file changed since the
//! closer's own last touch of it — derived from store-branch history (`bl-actor`
//! trailers; claim counts, so a claimant always has an anchor; no state) — AND
//! no matching seen-token is found. The refusal prints the unseen diff and
//! mints the token itself, so a bare retry passes: worst case anywhere is
//! exactly ONE refusal-with-diff, and a further edit refuses again with the new
//! diff (CAS semantics intact).
//!
//! The token is a file named for the ball id, content = the task file's blob
//! sha as displayed, minted by every `bl show <id>` — eager minting is safe
//! because a token can only ever SKIP a refusal whose content was just put on
//! someone's stdout, never cause one; stray tokens are inert. The mint home IS
//! the semantics: standing in a `work/<id>` worktree ⇒ that worktree's own
//! gitdir (per-agent scope — a writer's verify-after-edit `bl show` cannot
//! acknowledge on the claimant's behalf); anywhere else ⇒ the store clone's
//! gitdir, which always exists, so there is no git-or-not branch in core.
//! Tokens live only in territory bl owns (the XDG store clone, bl-created
//! worktree admin dirs) — never the userspace `.git` itself. Cleanup: worktree
//! tokens die with the teardown close already performs, a successful close
//! deletes the tokens it consumed, and `bl prime` sweeps store tokens naming
//! absent task files ([`sweep`] — absence is the closed-record, so a dead token
//! is self-identifying debris).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::git;

/// The token directory inside a gitdir bl owns. A gitdir, not the worktree:
/// teardown (`git worktree remove`) deletes the admin dir, so worktree tokens
/// need no sweep of their own.
const TOKEN_DIR: &str = "balls-seen";

/// Mint a seen-token for `id` after its content went to stdout (`bl show`).
/// Best-effort by design: a lost mint costs one refusal, never correctness, so
/// no read ever fails because a token could not be written. A dead ball mints
/// nothing — there is no task file at the store tip to acknowledge.
pub(crate) fn mint(invocation: &Path, store: &Path, id: &str) {
    let _ = try_mint(invocation, store, id);
}

/// The fallible mint body: resolve the blob at the store tip, then write the
/// token into the context-derived home ([`mint_home`]).
fn try_mint(invocation: &Path, store: &Path, id: &str) -> io::Result<()> {
    match head_blob(store, id) {
        Some(blob) => write_token(&mint_home(invocation, store)?, id, &blob),
        None => Ok(()), // dead or never-was: nothing at the tip to acknowledge
    }
}

/// The close-side CAS: pass (returning the token paths the close consumed —
/// empty when nothing changed) or refuse with the unseen diff, minting the
/// retry's token on the way out. The caller deletes the returned tokens only
/// after a successful seal ([`consume`]) — a failed close must leave them, or
/// the acknowledgment would be spent on nothing.
pub(crate) fn guard(store: &Path, invocation: &Path, id: &str, actor: &str) -> io::Result<Vec<PathBuf>> {
    let cur = head_blob(store, id).ok_or_else(|| {
        io::Error::other(format!("close {id}: tasks/{id}.md is not at the store tip — the store checkout is out of step; run `bl prime`"))
    })?;
    let anchor = anchor_commit(store, id, actor);
    if anchor.as_ref().is_some_and(|sha| blob_at(store, sha, id).as_deref() == Some(cur.as_str())) {
        return Ok(Vec::new()); // unchanged since the closer's own last touch
    }
    let tokens: Vec<PathBuf> = union(store, invocation, id)?
        .into_iter()
        .map(|home| home.join(TOKEN_DIR).join(id))
        .filter(|p| fs::read_to_string(p).is_ok_and(|c| c.trim() == cur))
        .collect();
    if !tokens.is_empty() {
        return Ok(tokens);
    }
    let diff = unseen_diff(store, anchor.as_deref(), id)?;
    write_token(&mint_home(invocation, store)?, id, &cur)?;
    Err(io::Error::other(format!(
        "close {id}: tasks/{id}.md changed since your last touch of it — the unseen diff:\n\n{diff}\n\
         the diff above is now acknowledged; re-run `bl close {id}` to seal exactly this content \
         (a further edit refuses again with its own diff)"
    )))
}

/// Delete the tokens a successful close consumed. Best-effort: the worktree
/// copies die with teardown anyway, and a leftover token for a closed ball is
/// inert debris `bl prime` sweeps.
pub(crate) fn consume(tokens: &[PathBuf]) {
    for token in tokens {
        let _ = fs::remove_file(token);
    }
}

/// `bl prime`'s sweep: delete store-gitdir tokens naming absent task files —
/// absence is the closed-record, so a dead token is self-identifying debris
/// (the prime-prunes-settled-state precedent, bl-292d). Worktree tokens are
/// not swept: their whole home dies with the worktree teardown.
pub(crate) fn sweep(store: &Path) -> io::Result<()> {
    let dir = gitdir(store)?.join(TOKEN_DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(()); // no tokens ever minted here
    };
    for entry in entries {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if !crate::taskfile::exists(store, &id) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Where a mint lands — the home IS the semantics (proof-of-sight scope):
/// standing in a `work/<id>` worktree ⇒ that worktree's own admin gitdir
/// (per-agent); anywhere else — repo root, subdirectory, non-git dir — ⇒ the
/// store clone's gitdir, which always exists, so gitless invocation takes the
/// identical path.
fn mint_home(invocation: &Path, store: &Path) -> io::Result<PathBuf> {
    match work_gitdir(invocation) {
        Some(dir) => Ok(dir),
        None => gitdir(store),
    }
}

/// The read union close consults: the store gitdir (always), the task's own
/// worktree admin dir — computed from the ID, never from cwd, as
/// `<project common gitdir>/worktrees/<id>` (§11: the worktree basename is the
/// id) — and the current work-worktree gitdir when standing in one. Misses are
/// meaningless; a hit only ever skips a refusal.
fn union(store: &Path, invocation: &Path, id: &str) -> io::Result<Vec<PathBuf>> {
    let mut homes = vec![gitdir(store)?];
    let common = rev_parse(invocation, "--git-common-dir").map(|c| c.join("worktrees").join(id));
    for home in [common, work_gitdir(invocation)].into_iter().flatten() {
        if !homes.contains(&home) {
            homes.push(home);
        }
    }
    Ok(homes)
}

/// The gitdir of a checkout bl owns (the store clone), as an error — unlike the
/// optional project-side probes, the store rung must exist for the mechanism to
/// hold.
fn gitdir(dir: &Path) -> io::Result<PathBuf> {
    rev_parse(dir, "--git-dir").ok_or_else(|| io::Error::other(format!("{}: not a git checkout — run `bl prime`", dir.display())))
}

/// The current checkout's admin gitdir iff `invocation` stands in a `work/<id>`
/// worktree — a LINKED worktree (gitdir ≠ common dir) on a `work/` branch. The
/// linked-only requirement is what keeps bl out of the userspace `.git`: a
/// `work/`-named branch checked out at the repo root is not bl territory.
fn work_gitdir(invocation: &Path) -> Option<PathBuf> {
    let head = git::run(invocation, &["rev-parse", "--abbrev-ref", "HEAD"], None).ok()?;
    if !head.trim().starts_with("work/") {
        return None;
    }
    let (dir, common) = (rev_parse(invocation, "--git-dir")?, rev_parse(invocation, "--git-common-dir")?);
    (dir != common).then_some(dir)
}

/// One absolute `git rev-parse` path probe, `None` where there is no repo.
fn rev_parse(dir: &Path, what: &str) -> Option<PathBuf> {
    let out = git::run(dir, &["rev-parse", "--path-format=absolute", what], None).ok()?;
    Some(PathBuf::from(out.trim()))
}

/// The blob sha of `tasks/<id>.md` at the store tip — the content `bl show`
/// displays and close seals. `None` when the tip carries no such file (a dead
/// ball, an unborn store).
fn head_blob(store: &Path, id: &str) -> Option<String> {
    blob_at(store, "HEAD", id)
}

/// The blob sha of `tasks/<id>.md` at any tree-ish, `None` when absent there.
fn blob_at(store: &Path, treeish: &str, id: &str) -> Option<String> {
    let out = git::run(store, &["rev-parse", &format!("{treeish}:tasks/{id}.md")], None).ok()?;
    Some(out.trim().to_string())
}

/// The closer's anchor: the newest store commit touching `tasks/<id>.md` whose
/// §5 `bl-actor` trailer is `actor` — their own last touch, derived from
/// history alone (claim counts, so a claimant always has one; no state).
/// `None` for a closer who never touched the ball: everything is unseen.
fn anchor_commit(store: &Path, id: &str, actor: &str) -> Option<String> {
    let path = format!("tasks/{id}.md");
    let format = "--format=%H%x1f%(trailers:key=bl-actor,valueonly,separator=%x2C)";
    let log = git::run(store, &["log", format, "--", &path], None).ok()?;
    log.lines().find_map(|line| {
        let (sha, by) = line.split_once('\u{1f}')?;
        (by.trim() == actor).then(|| sha.to_string())
    })
}

/// The diff the refusal shows: anchor → tip for the one task file. A closer
/// with no anchor diffs from the EMPTY tree — they have seen none of it, so
/// the whole file is the unseen content.
fn unseen_diff(store: &Path, anchor: Option<&str>, id: &str) -> io::Result<String> {
    let base = match anchor {
        Some(sha) => sha.to_string(),
        None => git::run(store, &["mktree"], Some(""))?.trim().to_string(),
    };
    git::run(store, &["diff", &base, "HEAD", "--", &format!("tasks/{id}.md")], None)
}

/// Write the token: `<home>/balls-seen/<id>`, content = the blob sha.
fn write_token(home: &Path, id: &str, blob: &str) -> io::Result<()> {
    let dir = home.join(TOKEN_DIR);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(id), format!("{blob}\n"))
}

#[cfg(test)]
#[path = "seen_tests.rs"]
mod tests;
