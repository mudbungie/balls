//! End-to-end enrollment: a satellite checkout joining a shared center through
//! the real `bl` + `tracker`. Both `prime --install` (adopt a center's config)
//! and `prime --center` (bl-35e5 one-shot enrollment: durable bind + adopt +
//! prime) run against a LOCAL bare center — a filesystem path is a legitimate
//! center (design `docs/design/bl-0161-cross-repo-work.md` §Q4), the same code
//! path as a hosted one. Split from `tests/dispatch.rs` (the 300-line cap) as the
//! cohesive center group; each `tests/*.rs` is its own crate, so the small
//! harness (`bl_primed`, `git`, `center`) is local to this file.

use assert_cmd::Command;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle lands in the tempdir, not the real `$HOME`.
/// The `tracker` sibling is found beside the built `bl` (§12).
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success — the harness builds the center
/// repo with plain git (no access to the crate-internal runner).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A BARE center carrying a `balls/config` branch whose `config/` names
/// `tasks_branch` and wires the tracker, plus a `# CENTER-MARKER` in `balls.toml`
/// so the adopting side can prove it copied the center's file verbatim.
fn center(dir: &Path) -> PathBuf {
    let bare = dir.join("center.git");
    git(dir, &["init", "--bare", "-q", "-b", "balls/config", &bare.to_string_lossy()]);
    let seed = dir.join("center-seed");
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "c"]);
    git(&seed, &["config", "user.email", "c@c"]);
    std::fs::create_dir_all(seed.join("config")).unwrap();
    std::fs::write(seed.join("config/balls.toml"), "tasks_branch = \"balls/tasks\"\n# CENTER-MARKER\n").unwrap();
    std::fs::write(
        seed.join("config/plugins.toml"),
        // prime.post wires the tracker's content-settle (founding push on a first
        // prime, then fetch-ff + publish) — without it a fresh clone never founds
        // the remote store (bl-0a23).
        "[hooks]\n\"sync.pre\" = [\"bl-tracker\"]\n\"prime.pre\" = [\"bl-tracker\"]\n\"prime.post\" = [\"bl-tracker\"]\n\"install.pre\" = [\"bl-tracker\"]\n",
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "center config"]);
    git(&seed, &["push", "-q", "origin", "balls/config"]);
    bare
}

/// The clone bundle (landing/store/binding) for an invocation at `project`.
fn clone_dir(state: &Path, project: &Path) -> balls::layout::CloneDir {
    balls::layout::Xdg::with(Path::new("/unused"), None, Some(&state.to_string_lossy())).clone_dir(project)
}

#[test]
fn prime_install_adopts_a_centers_config_via_the_tracker_fetch() {
    // §13 end to end: the tracker (install.pre) fetches the center's config, core
    // copies it into the landing, then prime+sync run. Proof the whole
    // engine→subprocess→tracker→core-copy path works — core itself never fetches.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    let bare = center(tmp.path());

    bl_primed(&project, &home, &state)
        .args(["prime", "--install", &bare.to_string_lossy()])
        .assert()
        .success()
        .stdout(contains("install:")); // the change summary prints

    // The landing's config is the center's, copied verbatim (the marker proves it).
    let cfg = std::fs::read_to_string(clone_dir(&state, &project).landing().join("config/balls.toml")).unwrap();
    assert!(cfg.contains("CENTER-MARKER"), "adopted the center's config file: {cfg}");
}

#[test]
fn prime_center_enrolls_a_satellite_into_a_local_bare_center() {
    // bl-35e5 (§Q3/§Q4) end to end: `bl prime --center BARE` from a satellite is
    // ONE-command enrollment — the durable per-clone binding write + config
    // adoption + prime, with no half-enrolled window. A LOCAL bare repo is a
    // legitimate center (§Q4: two repos on one box share through `git init --bare`,
    // the SAME code path as a hosted one). Proof: (1) the center's config is
    // adopted (the CENTER-MARKER); (2) a DURABLE binding is written (the enrollment
    // half `--install` lacks — a plain later op resolves the center with no flag);
    // (3) re-running converges.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    let bare = center(tmp.path()); // a bare repo — the local center

    bl_primed(&project, &home, &state)
        .args(["prime", "--center", &bare.to_string_lossy()])
        .assert()
        .success()
        .stdout(contains("install:")); // the adopt change summary prints

    let clone = clone_dir(&state, &project);
    // (1) The landing's config is the center's, copied verbatim.
    let cfg = std::fs::read_to_string(clone.landing().join("config/balls.toml")).unwrap();
    assert!(cfg.contains("CENTER-MARKER"), "adopted the center's config: {cfg}");
    // (2) The per-clone binding durably names the center (what `conf set
    // task-remote BARE` writes) — this is the difference from `--install`.
    let binding = std::fs::read_to_string(clone.binding()).unwrap();
    assert!(binding.contains(&*bare.to_string_lossy()), "durable binding to the center: {binding}");

    // (3) Re-running is prime-idempotent: the binding converges, install re-copies
    // identical bytes, sync fast-forwards — a plain prime (no --center) suffices,
    // because the durable binding now routes it.
    bl_primed(&project, &home, &state).arg("prime").assert().success();
}
