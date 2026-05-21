//! Code-repo provenance string for tasks (bl-499b, bl-7523).
//!
//! Two task fields need to record which code repo a piece of state
//! lives in: `task.repo` (creation provenance — set at `bl create`)
//! and `task.delivered_repo` (delivery provenance — set when `bl
//! review`/`bl close` lands the squash). Both pick the same shape —
//! the local clone's `origin` URL when there is one, else the repo's
//! basename, else the full path — so a reader on a shared hub can
//! resolve a sha or task back to its source repo without depending on
//! the per-clone `.git/config` of whoever wrote the field.
//!
//! Pure choice on `Option<String>` + `Path` so every branch is
//! unit-testable without spawning git. The `current` entry point is
//! the impure one: shells out to `git remote get-url origin` once.
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
    identity_from(git_origin_url(root), root)
}

fn git_origin_url(root: &Path) -> Option<String> {
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
