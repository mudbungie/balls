//! §12 trail pointer — `config/plugins/tracker/remote.toml` on the balls branch.
//!
//! Auto-discovery is transitive: a branch's pointer names the NEXT hop, so a
//! fresh clone of a code repo can DISCOVER a central task store it could not
//! name directly — it reads the `next:` the team committed onto `origin:balls`.
//! The pointer is therefore tracker territory and **committed** (it rides the
//! shared branch, which is what makes onboarding zero-touch); a single scalar
//! `next` URL, no `branch`/auth/ttl (branch is config or a `#frag`, auth is
//! git's, refresh is sync's). Absent ⇒ this branch is the trail's end.
//!
//! The tracker reads this file to resolve its upstream — the committed pointer
//! wins over the auto-discovered wire remote (§12). SETTING it is `bl prime`'s
//! job: [`write`] extends the trail (`--center <url>`, stealth→federated) and
//! [`clear`] truncates it (`--stealth`, federated→stealth). The pointer is
//! `prime`'s exclusively — `bl install` is pointer-excluded, so
//! capability-transfer can never re-home a checkout (§6). Core writes the file
//! here (the single source of its format) and commits it as config (§12).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The committed `next:` trail pointer. One optional scalar; an absent key (an
/// empty file) is a trail-end branch.
#[derive(Deserialize, Serialize)]
struct Pointer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next: Option<String>,
}

/// `<operating>/config/plugins/tracker/remote.toml` (§2 committed layout).
fn path(operating: &Path) -> PathBuf {
    operating
        .join("config")
        .join("plugins")
        .join("tracker")
        .join("remote.toml")
}

/// Read the `next:` hop from `operating`'s committed tracker config. An absent
/// file is the common, un-federated case — the trail ends here, so `None`.
pub fn read(operating: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path(operating)) {
        Ok(text) => Ok(toml::from_str::<Pointer>(&text).map_err(io::Error::other)?.next),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Set this checkout's `next:` hop to `url` — `bl prime --center`, which extends
/// the trail (§12). Creates `config/plugins/tracker/` if absent and replaces any
/// prior pointer (idempotent re-home). Core commits the file separately.
pub fn write(operating: &Path, url: &str) -> io::Result<()> {
    let file = path(operating);
    fs::create_dir_all(file.parent().expect("path has a parent"))?;
    let text = toml::to_string(&Pointer { next: Some(url.to_string()) }).map_err(io::Error::other)?;
    fs::write(file, text)
}

/// Truncate the trail — `bl prime --stealth`, removing the `next:` hop so this
/// checkout becomes its own terminus (§12). A no-op when already pointer-free,
/// so it converges; absence is the trail-end the [`read`] side already models.
pub fn clear(operating: &Path) -> io::Result<()> {
    match fs::remove_file(path(operating)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::fixtures::set_pointer;
    use tempfile::TempDir;

    #[test]
    fn an_absent_file_is_a_trail_end() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
    }

    #[test]
    fn reads_a_committed_next_hop() {
        let tmp = TempDir::new().unwrap();
        set_pointer(tmp.path(), "git@hub:central");
        assert_eq!(read(tmp.path()).unwrap().as_deref(), Some("git@hub:central"));
    }

    #[test]
    fn an_empty_pointer_file_has_no_next() {
        let tmp = TempDir::new().unwrap();
        let file = path(tmp.path());
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "").unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
    }

    #[test]
    fn a_read_error_other_than_absence_propagates() {
        let tmp = TempDir::new().unwrap();
        // remote.toml is a directory: read_to_string errors, but not NotFound.
        fs::create_dir_all(path(tmp.path())).unwrap();
        assert!(read(tmp.path()).is_err());
    }

    #[test]
    fn malformed_toml_is_an_error() {
        let tmp = TempDir::new().unwrap();
        let file = path(tmp.path());
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "next = [not a string]\n").unwrap();
        assert!(read(tmp.path()).is_err());
    }

    #[test]
    fn write_then_read_round_trips_the_hop_creating_the_dir() {
        let tmp = TempDir::new().unwrap();
        // No config/plugins/tracker/ yet — write creates it.
        write(tmp.path(), "git@hub:central").unwrap();
        assert_eq!(read(tmp.path()).unwrap().as_deref(), Some("git@hub:central"));
    }

    #[test]
    fn write_replaces_a_prior_pointer() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "git@hub:first").unwrap();
        write(tmp.path(), "git@hub:second").unwrap();
        assert_eq!(read(tmp.path()).unwrap().as_deref(), Some("git@hub:second"));
    }

    #[test]
    fn clear_truncates_to_a_trail_end_and_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "git@hub:central").unwrap();
        clear(tmp.path()).unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
        clear(tmp.path()).unwrap(); // already gone — converges, no error
    }

    #[test]
    fn clear_propagates_a_non_absence_error() {
        let tmp = TempDir::new().unwrap();
        // remote.toml is a directory: remove_file errors, but not NotFound.
        fs::create_dir_all(path(tmp.path())).unwrap();
        assert!(clear(tmp.path()).is_err());
    }
}
