//! Code-repo provenance string for tasks (bl-499b, bl-7523, bl-8994).
//!
//! Two task fields record which code repo a piece of state lives in,
//! and they pull from this module with different fallback discipline:
//!
//! - `task.delivered_repo` (delivery provenance — set when `bl
//!   review`/`bl close` lands the squash) uses `current`: `origin`
//!   URL, else basename, else path. A delivery always happened in
//!   some checked-out tree, so it always resolves to *something*.
//! - `task.repo` (code-home provenance) uses `origin_url`: the
//!   `origin` URL, or nothing. A bare basename is not fetchable from
//!   a sibling clone, and `repo` is re-anchored at `bl claim` anyway
//!   (bl-8994), so when there is no `origin` the honest value is
//!   null — not a guess no reader can act on.
//!
//! Pure choice on `Option<String>` + `Path` (`identity_from`) so
//! every branch is unit-testable without spawning git. `current` and
//! `origin_url` are the impure entry points: each shells out to `git
//! remote get-url origin` once.
//!
//! Forward-compat: `task.delivered_repo` is `Option<String>` with
//! `skip_serializing_if = None`. A task delivered by a pre-bl-7523
//! `bl` has no field; the agreed reading is "delivered against the
//! locally-checked-out repo," so single-repo deployments stay
//! byte-identical and a fresh delivery in a multi-repo deployment
//! carries provenance forward.

use std::path::Path;

/// Provenance string for the code repo rooted at `root`. The same
/// rule covers both `task.repo` (creation) and `task.delivered_repo`
/// (delivery): URL > basename > path. Always returns something so the
/// task field is never silently null when we have a repo to point at.
pub fn current(root: &Path) -> String {
    identity_from(origin_url(root), root)
}

/// The repo's fetchable `origin` URL, or `None` when it has no
/// `origin` remote. Unlike `current`, this never falls back to a
/// basename or path: a non-URL string is not something a reader on
/// another clone can `git fetch`, so callers that auto-write
/// `task.repo` (`bl create`, `bl claim`) take `None` and leave the
/// field null rather than persist an unusable value (bl-8994).
pub fn origin_url(root: &Path) -> Option<String> {
    let out = crate::git::clean_git_command(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// Pure choice: `origin` URL if present, else the repo's directory
/// name, else the full path. Split out so every branch is unit-tested
/// without spawning git.
pub fn identity_from(url: Option<String>, root: &Path) -> String {
    url.or_else(|| root.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn identity_prefers_url() {
        let got = identity_from(Some("git@h:proj.git".into()), Path::new("/x/y"));
        assert_eq!(got, "git@h:proj.git");
    }

    #[test]
    fn identity_falls_back_to_basename() {
        assert_eq!(identity_from(None, Path::new("/x/myrepo")), "myrepo");
    }

    #[test]
    fn identity_falls_back_to_path_when_no_basename() {
        assert_eq!(identity_from(None, Path::new("/")), "/");
    }

    #[test]
    fn current_in_non_git_dir_falls_back_to_basename() {
        // `git remote get-url origin` fails outside a repo, so the
        // impure entry point should land on the basename fallback.
        let dir = tempfile::TempDir::new().unwrap();
        let got = current(dir.path());
        let want = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(got, want);
    }
}
