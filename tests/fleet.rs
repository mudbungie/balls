//! End-to-end cross-repo FLEET scoping (bl-0161 Q2, bl-5965) through the real
//! `bl` + `bl-tracker`. Two distinct-root project checkouts enrolled on ONE
//! shared LOCAL bare center (a filesystem path is a legitimate center, design
//! `docs/design/bl-0161-cross-repo-work.md` §Q4) converge their task store, then:
//!
//!   * plain `bl list` shows only THIS checkout's own claim-admitted set — a
//!     foreign-rooted ball is hidden, a ROOTLESS ball (born off no git repo) is
//!     admitted everywhere;
//!   * `bl list --everywhere` lifts the scope and hangs a `  [<project>]` label on
//!     every foreign row, the name DECODED from the enrolled clone dir's basename;
//!   * a wrong-repo `bl claim` is refused (the shared `admits` guard);
//!   * a checkout at a path with spaces + non-ASCII round-trips through the
//!     percent-encoded `clones/<enc>/` dir name, and every op resolves it
//!     idempotently.
//!
//! Convergence uses the tracker's real adopt-then-fast-forward: the first repo
//! FOUNDS the store, a later enroll (`prime --center`) ADOPTS the established
//! history via `prime/pre` clone-in, and each `bl prime` thereafter imports (post
//! sync fetch-ff) then publishes. Every `tests/*.rs` is its own crate, so this
//! ~55-line harness is local to the file; the `tracker` sibling is found beside
//! the built `bl` (§12).

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the shared
/// `home`/`state` so every enrolled checkout lands in ONE clones dir (the fleet
/// view enumerates it), never the real `$HOME`. Inherited plugin-chain env is
/// scrubbed so a `bl`-under-test never reads this harness's own runner context.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success — the harness builds repos with
/// plain git (no access to the crate-internal runner).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project repo on `main` at `dir` with one seed commit — a distinct root
/// commit per repo, so each stamps its own `root_commit` on the balls it creates.
fn mkrepo(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main", &dir.to_string_lossy()]);
    git(dir, &["config", "user.name", "test"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    std::fs::write(dir.join("seed.txt"), dir.to_string_lossy().as_bytes()).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "seed"]);
    dir.to_path_buf()
}

/// A BARE center carrying a `balls/config` branch that names `tasks_branch` and
/// wires the tracker on the sync/prime/install hooks — the same fixture the
/// enrollment E2E uses, so a satellite's `prime --center` adopts + founds/imports.
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
        "[hooks]\n\"sync.pre\" = [\"bl-tracker\"]\n\"prime.pre\" = [\"bl-tracker\"]\n\
         \"prime.post\" = [\"bl-tracker\"]\n\"install.pre\" = [\"bl-tracker\"]\n",
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "center config"]);
    git(&seed, &["push", "-q", "origin", "balls/config"]);
    bare
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// `bl list <extra…>` stdout as a String.
fn list(project: &Path, home: &Path, state: &Path, extra: &[&str]) -> String {
    let out = bl(project, home, state).arg("list").args(extra).assert().success();
    String::from_utf8(out.get_output().stdout.clone()).unwrap()
}

/// Enroll `project` on `bare`, then (idempotently) import + publish the store.
/// `prime --center` durably binds + adopts an established store; a plain `prime`
/// afterwards imports peers' pushes (post fetch-ff) and publishes local writes.
fn enroll(project: &Path, home: &Path, state: &Path, bare: &Path) {
    bl(project, home, state).args(["prime", "--center", &bare.to_string_lossy()]).assert().success();
}

/// Create `title` in `project` and publish it to the center (a plain `prime`
/// fast-forwards the shared store with the new commit).
fn create_publish(project: &Path, home: &Path, state: &Path, title: &str) -> String {
    let id = created_id(bl(project, home, state).args(["create", title, "--as", "me"]).assert().success());
    bl(project, home, state).arg("prime").assert().success();
    id
}

/// The single line of `out` that mentions `id` — panics if absent (proves the
/// row is present AND lets us inspect its trailing fleet-view label).
fn row_of<'a>(out: &'a str, id: &str) -> &'a str {
    out.lines().find(|l| l.contains(id)).unwrap_or_else(|| panic!("no row for {id} in:\n{out}"))
}

#[test]
fn plain_list_scopes_to_this_root_and_everywhere_labels_the_foreign_rows() {
    // Two distinct-root repos + a non-git box, all enrolled on one center. Repo A
    // founds the store and publishes A; B adopts it, publishes B; a rootless box
    // adopts + publishes R. After A re-primes (imports both), A's plain list is
    // EXACTLY its admitted set (own A + rootless R, foreign B hidden), and
    // `--everywhere` surfaces B with a decoded `[repob]` label.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    std::fs::create_dir_all(&home).unwrap();
    let bare = center(tmp.path());

    let repo_a = mkrepo(&tmp.path().join("repoa"));
    let repo_b = mkrepo(&tmp.path().join("repob"));
    let plainbox = tmp.path().join("plainbox"); // NON-git: its balls are rootless
    std::fs::create_dir_all(&plainbox).unwrap();

    enroll(&repo_a, &home, &state, &bare);
    let id_a = create_publish(&repo_a, &home, &state, "Ball in A");

    enroll(&repo_b, &home, &state, &bare); // adopts A's established store
    let id_b = create_publish(&repo_b, &home, &state, "Ball in B");

    enroll(&plainbox, &home, &state, &bare); // a non-git checkout — rootless balls
    let id_r = create_publish(&plainbox, &home, &state, "Rootless ball");

    // A imports every peer push, then reads its OWN scope.
    bl(&repo_a, &home, &state).arg("prime").assert().success();
    let plain = list(&repo_a, &home, &state, &[]);
    assert!(plain.contains(&id_a), "own ball shown:\n{plain}");
    assert!(plain.contains(&id_r), "rootless ball admitted everywhere:\n{plain}");
    assert!(!plain.contains(&id_b), "foreign-rooted ball HIDDEN from plain list:\n{plain}");

    // `--everywhere` lifts the scope: all three rows, and the foreign B row alone
    // carries the `  [repob]` label decoded from its enrolled clone dir basename.
    let ew = list(&repo_a, &home, &state, &["--everywhere"]);
    assert!(ew.contains(&id_a) && ew.contains(&id_b) && ew.contains(&id_r), "all rows present:\n{ew}");
    assert!(row_of(&ew, &id_b).contains("[repob]"), "foreign row labeled by clone dir:\n{ew}");
    assert!(!row_of(&ew, &id_a).contains('['), "the home row earns no label:\n{ew}");
    assert!(!row_of(&ew, &id_r).contains('['), "the rootless row earns no label:\n{ew}");

    // Off the fleet view, no row ever carries a `[label]`.
    assert!(!plain.contains('['), "plain list never labels a row:\n{plain}");

    // The wrong-repo claim is refused by the shared `admits` guard: B belongs to
    // repo B's root, and A is a different checkout.
    bl(&repo_a, &home, &state)
        .args(["claim", &id_b, "--as", "me"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("belongs to the project rooted at"));

    // Symmetry: from B, A is the foreign one — plain list hides it.
    let b_plain = list(&repo_b, &home, &state, &[]);
    assert!(b_plain.contains(&id_b) && !b_plain.contains(&id_a), "B hides A:\n{b_plain}");
}

#[test]
fn a_unicode_and_space_checkout_round_trips_the_encoded_clone_dir_idempotently() {
    // A project path with a space AND non-ASCII enrolls on the center; its clone
    // bundle lives under a percent-ENCODED `clones/<enc>/` dir, yet every op
    // resolves it, its own list is unscathed, and `--everywhere` from a peer
    // DECODES the dir name back to the exact basename for the label. Re-running
    // ops mints no second clone dir (idempotent resolution, bl-5965).
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    std::fs::create_dir_all(&home).unwrap();
    let bare = center(tmp.path());

    let repo_a = mkrepo(&tmp.path().join("repoa"));
    let fancy_name = "prôje ct café"; // space + non-ASCII → must percent-encode
    let fancy = mkrepo(&tmp.path().join(fancy_name));

    enroll(&repo_a, &home, &state, &bare);
    let id_a = create_publish(&repo_a, &home, &state, "Ball in A");

    enroll(&fancy, &home, &state, &bare);
    let id_f = create_publish(&fancy, &home, &state, "Ball in fancy");

    // The clone bundle dir is percent-encoded — the raw name never appears, the
    // encoded one does (a space becomes %20, so no literal space in the entry).
    let clones = state.join("balls/clones");
    let names: Vec<String> =
        std::fs::read_dir(&clones).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 2, "exactly two enrolled clone dirs: {names:?}");
    assert!(names.iter().any(|n| n.contains("%20")), "the space is percent-encoded (%20): {names:?}");
    assert!(!names.iter().any(|n| n.contains(fancy_name)), "the raw path never appears verbatim: {names:?}");
    assert!(!names.iter().any(|n| n.contains(' ')), "no literal space survives encoding: {names:?}");

    // The fancy checkout resolves its OWN store fine: plain list shows its ball,
    // hides A's (foreign). A peer's `--everywhere` DECODES the dir name back to
    // the exact unicode+space basename for the label.
    let f_plain = list(&fancy, &home, &state, &[]);
    assert!(f_plain.contains(&id_f) && !f_plain.contains(&id_a), "fancy sees only its own:\n{f_plain}");

    bl(&repo_a, &home, &state).arg("prime").assert().success(); // A imports fancy's push
    let ew = list(&repo_a, &home, &state, &["--everywhere"]);
    assert!(row_of(&ew, &id_f).contains(&format!("[{fancy_name}]")), "label decodes to the exact name:\n{ew}");

    // Idempotent resolution: repeated ops from the fancy path mint no new clone
    // dir and keep resolving the same store (the encode↔decode round-trips).
    bl(&fancy, &home, &state).arg("prime").assert().success();
    let f_plain2 = list(&fancy, &home, &state, &[]);
    assert!(f_plain2.contains(&id_f), "still resolves its store after a re-prime:\n{f_plain2}");
    let after = std::fs::read_dir(&clones).unwrap().count();
    assert_eq!(after, 2, "no second clone dir minted for the same path");

    // And the wrong-repo claim is still refused across the encoded boundary.
    bl(&repo_a, &home, &state)
        .args(["claim", &id_f, "--as", "me"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("belongs to the project rooted at"));
}
