//! `doctor`'s repo-config validation, split per layout. Lifted out of
//! `doctor.rs` for line-budget. Legacy clones validate the committed
//! `.balls/config.json` schema; XDG clones validate the tracker-branch
//! `repo.json` / `project.json` pair. Stealth XDG clones have no
//! tracker branch to validate against.

use crate::config::Config;
use crate::doctor::Finding;
use crate::project_config::ProjectConfig;
use crate::repo_json::RepoJson;
use crate::store::{Layout, Store};
use std::path::Path;

pub(crate) fn check_config(store: &Store, out: &mut Vec<Finding>) {
    match store.layout {
        Layout::Legacy => check_config_legacy(store, out),
        Layout::Xdg => check_config_xdg(store, out),
    }
}

fn check_config_legacy(store: &Store, out: &mut Vec<Finding>) {
    if let Err(e) = Config::load(&store.config_path()) {
        out.push(Finding::flag(
            format!("config at {} is unreadable: {e}", store.config_path().display()),
            "config.json is committed to main — restore it with \
             `git checkout main -- .balls/config.json`",
        ));
    }
}

/// XDG layout splits `Config` into `repo.json` (per-repo) and
/// `project.json` (project-wide), both on the tracker branch. Doctor
/// validates each layer can be parsed; an absent `repo.json` is
/// `RepoJson::read_or_default`-defaulted and not drift. Stealth clones
/// have no tracker branch — no layers to validate.
fn check_config_xdg(store: &Store, out: &mut Vec<Finding>) {
    if let Some(f) = xdg_config_finding(
        store.stealth,
        &store.config_path(),
        &store.project_config_path(),
    ) {
        out.push(f);
    }
}

/// Pure helper: validate the XDG-layout config layers at the given
/// paths. Stealth clones (no tracker branch) short-circuit to `None`.
/// Returns the first finding (`repo.json` first, then `project.json`)
/// or `None` when both parse. Extracted so the corrupt-layer and
/// stealth-skip arms have direct unit tests without spinning up a
/// migrated clone fixture.
pub(crate) fn xdg_config_finding(
    stealth: bool,
    repo_path: &Path,
    project_path: &Path,
) -> Option<Finding> {
    if stealth {
        return None;
    }
    if let Err(e) = RepoJson::read_or_default(repo_path) {
        return Some(Finding::flag(
            format!("repo.json at {} is unreadable: {e}", repo_path.display()),
            "repo.json lives on the tracker `balls/tasks` branch — \
             restore it from the tracker checkout's git history",
        ));
    }
    if let Err(e) = ProjectConfig::resolve(project_path, repo_path) {
        return Some(Finding::flag(
            format!("project.json at {} is unreadable: {e}", project_path.display()),
            "project.json lives on the tracker `balls/tasks` branch — \
             restore it from the tracker checkout's git history",
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::xdg_config_finding;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn both_absent_reads_as_defaults_and_is_silent() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo.json");
        let project = dir.path().join("project.json");
        assert!(xdg_config_finding(false, &repo, &project).is_none());
    }

    #[test]
    fn stealth_short_circuits_to_silent_even_with_garbage_paths() {
        // A stealth XDG clone has no tracker branch; the paths it
        // would consult never get materialized. The helper must
        // short-circuit before touching disk.
        let f = xdg_config_finding(
            true,
            std::path::Path::new("/no/such/repo.json"),
            std::path::Path::new("/no/such/project.json"),
        );
        assert!(f.is_none());
    }

    #[test]
    fn corrupt_repo_json_is_flagged_first() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo.json");
        let project = dir.path().join("project.json");
        fs::write(&repo, "{ not json").unwrap();
        let f = xdg_config_finding(false, &repo, &project).unwrap();
        assert!(f.problem.contains("repo.json"));
        assert!(f.problem.contains("is unreadable"));
        assert!(f.hint.as_ref().unwrap().contains("tracker"));
    }

    #[test]
    fn corrupt_project_json_is_flagged_when_repo_parses() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo.json");
        let project = dir.path().join("project.json");
        fs::write(&repo, "{}").unwrap();
        fs::write(&project, "{ not json").unwrap();
        let f = xdg_config_finding(false, &repo, &project).unwrap();
        assert!(f.problem.contains("project.json"));
        assert!(f.problem.contains("is unreadable"));
    }
}
