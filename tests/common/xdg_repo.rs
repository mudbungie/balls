//! `XdgRepo` fixture for tests that need a freshly-XDG-initialized
//! clone backed by a bare remote. Split out of `common/mod.rs` to keep
//! it under the 300-line cap; `mod.rs` re-exports both items so call
//! sites keep using `common::{XdgRepo, new_xdg_repo}`.

#![allow(dead_code)]

use super::{clone_from_remote, git, new_bare_remote, test_home_path, xdg_init, Repo};
use std::path::Path;

/// XDG-initialized clone backed by a fresh bare remote. Owning both
/// the clone and the remote keeps the remote's tempdir alive for the
/// lifetime of the fixture.
pub struct XdgRepo {
    pub remote: Repo,
    pub clone: Repo,
}

impl XdgRepo {
    pub fn path(&self) -> &Path {
        self.clone.path()
    }
}

/// Materialize a fresh XDG-mode clone via `Store::init_xdg`. A bare
/// remote acts as `origin` so the tracker checkout lands at the
/// documented XDG path; the per-thread test HOME is the discovery
/// root, so subsequent `bl(repo.path())` calls — which set HOME on
/// the subprocess to that same `test_home_path()` — see the XDG state
/// this helper just wrote.
///
/// The clone gets a seed commit on `main` pushed to the bare remote
/// before `init_xdg` runs. XDG init never writes on main (SPEC §14.19),
/// so without this the clone has an unborn HEAD and `bl claim`/`bl
/// review` fail when they read the current branch.
pub fn new_xdg_repo() -> XdgRepo {
    let remote = new_bare_remote();
    let clone = clone_from_remote(remote.path(), "xdg");
    std::fs::write(clone.path().join("README"), "seed\n").unwrap();
    git(clone.path(), &["add", "README"]);
    git(clone.path(), &["commit", "-m", "seed", "--no-verify"]);
    git(clone.path(), &["push", "-q", "origin", "main"]);
    let home = test_home_path();
    xdg_init::init_xdg(clone.path(), &home, false, None);
    XdgRepo { remote, clone }
}
