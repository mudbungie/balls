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

/// A center like [`center`] whose committed config ALSO declares the stealth
/// sentinel (`task_remote = "none"`, the §12 rung-2 policy) — team-wide "no store
/// remote, on purpose". An adopting satellite inherits it like any team policy.
fn stealth_center(dir: &Path) -> PathBuf {
    let bare = dir.join("scenter.git");
    git(dir, &["init", "--bare", "-q", "-b", "balls/config", &bare.to_string_lossy()]);
    let seed = dir.join("scenter-seed");
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "c"]);
    git(&seed, &["config", "user.email", "c@c"]);
    std::fs::create_dir_all(seed.join("config")).unwrap();
    std::fs::write(
        seed.join("config/balls.toml"),
        "tasks_branch = \"balls/tasks\"\ntask_remote = \"none\"\n# CENTER-MARKER\n",
    )
    .unwrap();
    std::fs::write(
        seed.join("config/plugins.toml"),
        "[hooks]\n\"sync.pre\" = [\"bl-tracker\"]\n\"prime.pre\" = [\"bl-tracker\"]\n\"prime.post\" = [\"bl-tracker\"]\n\"install.pre\" = [\"bl-tracker\"]\n",
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "stealth center config"]);
    git(&seed, &["push", "-q", "origin", "balls/config"]);
    bare
}

/// `git -C <cwd> <args>` capturing trimmed stdout (a `for-each-ref` snapshot).
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn a_center_declaring_stealth_makes_a_satellite_stealth_despite_a_pushable_origin() {
    // The composition the docs assert but leave untested: the stealth sentinel
    // "travels on install like any team policy". `--center` writes a durable binding
    // to the center (rung 3), THEN `adopt` destructively copies the center's config
    // in — re-introducing `task_remote = "none"` at rung 2, which OUTRANKS the
    // binding. So enrollment AND every later op resolve stealth: no push anywhere,
    // even with a pushable `origin` sitting as bait.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@t"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    // The BAIT: a reachable, pushable origin. A stealth leak would found `balls/tasks` here.
    let origin = tmp.path().join("origin.git");
    git(tmp.path(), &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    git(&project, &["remote", "add", "origin", &origin.to_string_lossy()]);
    let center = stealth_center(tmp.path());

    // Enroll: durable bind to the center + adopt its stealth-declaring config + prime.
    bl_primed(&project, &home, &state)
        .args(["prime", "--center", &center.to_string_lossy()])
        .assert()
        .success();

    let clone = clone_dir(&state, &project);
    // (1) The sentinel travelled on install — the adopted landing carries it verbatim.
    let cfg = std::fs::read_to_string(clone.landing().join("config/balls.toml")).unwrap();
    assert!(cfg.contains("task_remote = \"none\""), "adopted the stealth sentinel: {cfg}");
    // Enrollment itself founded nothing on the bait origin nor pushed the center.
    assert!(!git_out(&origin, &["for-each-ref"]).contains("balls/tasks"), "enrollment founded no balls/tasks on origin");
    let origin_before = git_out(&origin, &["for-each-ref"]);
    let center_before = git_out(&center, &["for-each-ref"]);
    // The op writes no binding of its OWN — snapshot the enrollment binding to prove it inert.
    let binding_before = std::fs::read_to_string(clone.binding()).unwrap();

    // (2) A later MUTATING op stays stealth: create founds/pushes nowhere.
    bl_primed(&project, &home, &state).args(["create", "Local ball", "--as", "me"]).assert().success();

    assert_eq!(git_out(&origin, &["for-each-ref"]), origin_before, "the sentinel keeps the bait origin untouched");
    assert_eq!(git_out(&center, &["for-each-ref"]), center_before, "no push to the center either");
    // The ball is real, and it landed in the LOCAL store only.
    let store = clone.store();
    assert!(git_out(&store, &["log", "-1", "--format=%B", "balls/tasks"]).contains("Local ball"), "ball in the local store");
    // (3) The mutating op wrote NO durable binding of its own — the only binding is
    // `--center`'s enrollment write, byte-identical across the stealth op.
    assert_eq!(std::fs::read_to_string(clone.binding()).unwrap(), binding_before, "the stealth op wrote no new binding");
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
