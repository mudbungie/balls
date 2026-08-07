//! bl-chore's §14 SCRATCH — the record of what ONE claim minted, so the
//! rollback can take it back down (bl-ffbf).
//!
//! The plugin's only side effect is a NESTED `bl create`: a balls op with its
//! own commit point, sealed (and, in a tracked checkout, pushed) OUTSIDE the
//! claiming op's atom. When the claim then aborts, that child is the §14
//! appendix case — an artifact keyed to an op that never sealed, which nothing
//! ever converges onto — so the rollback must delete it, exactly as the jira
//! example deletes its orphan ticket.
//!
//! To delete it the rollback must KNOW it, and the ids cannot travel any other
//! way: §7 has no return channel and `BALLS_*` never crosses from one plugin
//! process to its later rollback process (§14). So they are written here, in the
//! plugin's own §1 territory, keyed by checkout and claimed ball — §14's
//! id-keyed scratch, the sanctioned `claim.post` writes / rollback reads
//! channel. The record is REWRITTEN (never appended) as each child lands, so it
//! always names exactly this claim's mints: a stale record from an earlier claim
//! of the same ball is overwritten by the first one, never inherited — a
//! rollback is scoped to ONE op invocation and must not reach back into a claim
//! that already succeeded.
//!
//! Two things end a record, and until bl-f88b only the first was built: the
//! rollback CONSUMES it ([`Minted::unwind`]), and the ball's own `close.post`
//! DISCARDS it ([`Minted::discard`]) — §14 bounds scratch lifetime by the
//! resource, and close is that terminal op. The old reading, "a successful
//! claim's record is inert bytes the next claim of that ball overwrites," holds
//! only for a RE-claim; a ball is normally claimed once, so what it described in
//! practice was a directory nothing would ever overwrite, read, or delete.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::Bl;
use crate::encoding::percent_encode;

/// The scratch for one (checkout, claimed ball) pair.
pub(super) struct Minted {
    dir: PathBuf,
}

/// `<territory>/<pct-enc invocation_path>/<id>/` — the §1 plugin territory,
/// per-CHECKOUT (the ball it names lives in that checkout's store) then
/// id-keyed, the same shape §11's delivery worktree uses.
pub(super) fn at(territory: &Path, invocation: &str, id: &str) -> Minted {
    Minted { dir: territory.join(percent_encode(invocation)).join(id) }
}

impl Minted {
    /// The file holding the ids, one per line.
    fn file(&self) -> PathBuf {
        self.dir.join("children")
    }

    /// Rewrite the record to exactly the children this claim has minted so far.
    /// Called after EACH mint, so a create that fails mid-list can still unwind
    /// the ones that landed.
    pub(super) fn record(&self, ids: &[String]) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.file(), ids.join("\n"))
    }

    /// Close every recorded child, then drop the record. `close` IS the delete
    /// here: a closed ball's `tasks/<id>.md` is REMOVED (§2, no archive dir), so
    /// the gate stops gating and stops appearing — and in a tracked checkout the
    /// nested close publishes that removal, which is the one place the orphan
    /// actually persists (core's own un-seal resets the local store branch past
    /// both the claim and the nested create). Idempotent: no record ⇒ nothing was
    /// minted ⇒ nothing to undo, which is also the guarded-bail and the
    /// nothing-configured case.
    pub(super) fn unwind(&self, cwd: &Path, actor: &str, bl: &dyn Bl) -> io::Result<()> {
        let Ok(recorded) = fs::read_to_string(self.file()) else {
            return Ok(());
        };
        for child in recorded.lines() {
            bl.run(cwd, &["close".to_string(), child.to_string(), "--as".to_string(), actor.to_string()])?;
        }
        self.discard()
    }

    /// Drop the record WITHOUT acting on what it names — the success-path end of
    /// life, and the tail of [`Minted::unwind`] so deletion has one home. Absent
    /// is the ordinary case (a ball claimed before bl-chore was wired, one whose
    /// guards minted nothing, a re-close), so `NotFound` is `Ok`.
    pub(super) fn discard(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            done => done,
        }
    }
}
