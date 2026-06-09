//! Throwaway git fixtures for the tracker's handler tests (§13). Every tracker
//! act is a real git op against a real remote, so the tests are too: a bare
//! repo stands in for the remote and a checkout for the store, exercised on
//! tempdirs so no test touches the dev repo. Shared here because sync, push and
//! prime all need the same shapes.

use super::git::git;
use super::payload::Binding;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The state branch name every fixture uses (§2 names it `balls`).
pub const BRANCH: &str = "balls";

/// Run a fixture git command, asserting success (setup must not fail silently).
fn run(cwd: &Path, args: &[&str]) {
    git(cwd, args).unwrap();
}

/// Configure a commit identity in `repo` so `git commit` works headlessly.
fn identify(repo: &Path) {
    run(repo, &["config", "user.name", "test"]);
    run(repo, &["config", "user.email", "test@example.com"]);
}

/// Commit `content` to `file` in `repo`, returning the new HEAD sha.
pub fn commit(repo: &Path, file: &str, content: &str) -> String {
    fs::write(repo.join(file), format!("{content}\n")).unwrap();
    run(repo, &["add", "-A"]);
    run(repo, &["commit", "-q", "-m", content]);
    git(repo, &["rev-parse", "HEAD"]).unwrap()
}

/// `repo`'s tip of `rev` (a branch on a bare remote, or `HEAD`).
pub fn tip(repo: &Path, rev: &str) -> String {
    git(repo, &["rev-parse", rev]).unwrap()
}

/// An empty bare remote on the `balls` branch — the bootstrap-on-miss case
/// (the branch does not exist on it yet). The name is uniquely numbered so a
/// test that builds two remotes in one tempdir gets two distinct repos, never
/// one path aliased — defensive uniqueness for any multi-remote fixture (bl-6a39).
pub fn empty_remote(tmp: &Path) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let remote = tmp.join(format!("remote-{}.git", N.fetch_add(1, Ordering::Relaxed)));
    run(tmp, &["init", "--bare", "-q", "-b", BRANCH, &remote.to_string_lossy()]);
    remote
}

/// An empty bare remote whose `pre-receive` hook always fails — any push is
/// rejected, while `ls-remote` still reports the (absent) branch. Models a box
/// with no write access: prime founds-on-miss, the push is denied, and §12 says
/// fall back to stealth-local silently.
pub fn unpushable_remote(tmp: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let remote = empty_remote(tmp);
    let hook = remote.join("hooks").join("pre-receive");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    remote
}

/// A bare remote carrying a `balls/config` landing branch whose `config/balls.toml`
/// names `store_branch` as its `tasks_branch` — the input for the §12
/// seeded-default-mismatch warning. Carries no `balls/tasks` store branch.
pub fn remote_with_config(tmp: &Path, store_branch: &str) -> PathBuf {
    let remote = empty_remote(tmp);
    let seed = tmp.join(format!("{}-cfgseed", remote.file_name().unwrap().to_string_lossy()));
    run(tmp, &["clone", "-q", &remote.to_string_lossy(), &seed.to_string_lossy()]);
    identify(&seed);
    run(&seed, &["checkout", "-q", "--orphan", crate::LANDING_BRANCH]);
    fs::create_dir_all(seed.join("config")).unwrap();
    fs::write(seed.join("config/balls.toml"), format!("tasks_branch = \"{store_branch}\"\n")).unwrap();
    run(&seed, &["add", "-A"]);
    run(&seed, &["commit", "-q", "-m", "config"]);
    run(&seed, &["push", "-q", "origin", crate::LANDING_BRANCH]);
    remote
}

/// A bare remote already carrying one commit on the `balls` branch — the
/// established case (adopt / sync / push).
pub fn remote_with_branch(tmp: &Path) -> PathBuf {
    let remote = empty_remote(tmp);
    let seed = tmp.join("seed");
    run(tmp, &["clone", "-q", &remote.to_string_lossy(), &seed.to_string_lossy()]);
    identify(&seed);
    commit(&seed, "seed.txt", "seed");
    run(&seed, &["push", "-q", "origin", BRANCH]);
    remote
}

/// A `name`d checkout of `remote`'s `balls` branch with a commit identity set.
pub fn checkout(tmp: &Path, remote: &Path, name: &str) -> PathBuf {
    let dest = tmp.join(name);
    run(tmp, &["clone", "-q", &remote.to_string_lossy(), &dest.to_string_lossy()]);
    identify(&dest);
    dest
}

/// A checkout of `remote`'s `balls` branch — the store for an established
/// remote.
pub fn store_clone(tmp: &Path, remote: &Path) -> PathBuf {
    checkout(tmp, remote, "store")
}

/// A fresh `balls`-branch checkout with one commit and nothing pushed — what
/// core hands the tracker to FOUND an absent remote (bootstrap-on-miss).
pub fn local_unpushed(tmp: &Path) -> PathBuf {
    let store = tmp.join("store");
    run(tmp, &["init", "-q", "-b", BRANCH, &store.to_string_lossy()]);
    identify(&store);
    commit(&store, "seed.txt", "seed");
    store
}

/// A local LANDING repo with NO store (`balls`) branch yet — what core founds
/// before `prime/pre` clones a remote store in (bl-0a23). Uniquely numbered so two
/// in one tempdir never alias (bl-6a39). Its own branch is `main`, deliberately
/// NOT under `balls/`: the fixtures' short `balls` store name would D/F-conflict
/// with a `balls/config` landing ref (in production the two are siblings,
/// `balls/tasks` + `balls/config`). Distinct from [`local_unpushed`], which sits
/// ON the store branch.
pub fn landing_repo(tmp: &Path) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let landing = tmp.join(format!("landing-{}", N.fetch_add(1, Ordering::Relaxed)));
    run(tmp, &["init", "-q", "-b", "main", &landing.to_string_lossy()]);
    identify(&landing);
    commit(&landing, "config.txt", "config");
    landing
}

/// An [`Env`](super::Env) whose XDG state root is under `state` (so a test's
/// clone bundle and stealth lock land in its tempdir, not the real `$HOME`).
pub fn env(home: &Path, state: &Path) -> super::Env {
    super::Env {
        xdg: crate::layout::Xdg::with(home, None, Some(&state.to_string_lossy())),
    }
}

/// A [`Binding`] over the `store` checkout, with `remote` present (tracked) or
/// absent (stealth). `invocation_path` doubles as `store` — the tests that care
/// about it set it explicitly.
pub fn binding(remote: Option<&Path>, store: &Path) -> Binding {
    Binding {
        remote: remote.map(|r| r.to_string_lossy().into_owned()),
        stealth: false,
        tasks_branch: BRANCH.to_string(),
        store: store.to_string_lossy().into_owned(),
        landing: String::new(),
        invocation_path: store.to_string_lossy().into_owned(),
    }
}

/// A [`Binding`] whose `tasks_branch` is the SEEDED DEFAULT (`balls/tasks`) — the
/// precondition for the §12 store-elsewhere warning; otherwise like [`binding`].
pub fn default_binding(remote: Option<&Path>, store: &Path) -> Binding {
    Binding { tasks_branch: crate::DEFAULT_TASKS_BRANCH.to_string(), ..binding(remote, store) }
}

/// A tracked [`Binding`] with `landing` set — what `prime/pre` needs, since its
/// clone-in and store-elsewhere read the landing repo (the store may not exist
/// yet on a first prime, bl-0a23).
pub fn tracked(remote: &Path, store: &Path, landing: &Path) -> Binding {
    Binding { landing: landing.to_string_lossy().into_owned(), ..binding(Some(remote), store) }
}
