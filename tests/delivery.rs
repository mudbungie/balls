//! End-to-end harness for the `bl-delivery` plugin binary (§11): build it and
//! drive a full claim→work→close lifecycle by subprocess, exactly as balls'
//! `plugin::Subprocess` would — §7 payload on stdin, §6 env, `<op> <phase>`
//! argv. The git all happens on throwaway repos in a temp dir.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use balls::delivery::worktree_path;
use balls::layout::Xdg;
use predicates::str::contains;
use tempfile::TempDir;

/// Run `git <args>` in `cwd`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    Command::new("git").current_dir(cwd).args(args).assert().success();
}

/// A throwaway project repo on `main` with a seed commit.
fn project(tmp: &Path) -> std::path::PathBuf {
    let root = tmp.join("proj");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "test"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "seed"]);
    root
}

/// The `bl-delivery` binary, wired with the §6 env, run from `cwd`.
fn delivery(cwd: &Path, home: &Path, op: &str, phase: &str, stdin: &str) -> Command {
    let mut cmd = Command::cargo_bin("bl-delivery").unwrap();
    cmd.current_dir(cwd)
        .env("BALLS_PLUGIN_NAME", "delivery")
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .args([op, phase])
        .write_stdin(stdin.to_string());
    cmd
}

fn post(invocation: &str, id: &str, title: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{invocation}"}},"current_state":{{"title":"{title}"}},"metadata":{{"bl-id":["{id}"]}}}}"#
    )
}

fn pre(invocation: &str, title: &str) -> String {
    format!(r#"{{"binding":{{"invocation_path":"{invocation}"}},"current_state":{{"title":"{title}"}}}}"#)
}

/// A `prime` diffless wire (§13): the actor + the binding's invocation and
/// terminus checkout. No ball state — prime authors none.
fn prime(actor: &str, invocation: &str, operating: &str) -> String {
    format!(r#"{{"actor":"{actor}","binding":{{"invocation_path":"{invocation}","operating":"{operating}"}}}}"#)
}

/// Write a `tasks/<id>.md` ball with `claimant` into `operating`.
fn claimed_ball(operating: &Path, id: &str, claimant: &str) {
    let tasks = operating.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(
        tasks.join(format!("{id}.md")),
        format!("+++\ntitle = \"t\"\ncreated = 0\nupdated = 0\nclaimant = \"{claimant}\"\n+++\n"),
    )
    .unwrap();
}

#[test]
fn protocol_self_describes_without_env_or_stdin() {
    Command::cargo_bin("bl-delivery")
        .unwrap()
        .arg("protocol")
        .assert()
        .success()
        .stdout(contains(r#""ops":["claim","unclaim","drop","close","prime"]"#));
}

#[test]
fn prime_re_materializes_only_the_actors_still_claimed_worktrees() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();
    let operating = tmp.path().join("operating");
    claimed_ball(&operating, "bl-mine", "me");
    claimed_ball(&operating, "bl-theirs", "you"); // another actor — left alone

    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let mine = worktree_path(&xdg, "delivery", inv, "bl-mine");
    let theirs = worktree_path(&xdg, "delivery", inv, "bl-theirs");

    delivery(&root, &home, "prime", "post", &prime("me", inv, operating.to_str().unwrap()))
        .assert()
        .success();

    assert!(mine.join("seed.txt").exists()); // my claim re-materialized
    assert!(!theirs.exists()); // a different actor's claim is not mine to make

    // Idempotent: a second prime over the same set converges to a no-op.
    delivery(&root, &home, "prime", "post", &prime("me", inv, operating.to_str().unwrap()))
        .assert()
        .success();
    assert!(mine.join("seed.txt").exists());
}

#[test]
fn prime_without_a_terminus_in_the_wire_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let payload = r#"{"actor":"me","binding":{"invocation_path":"/proj"}}"#;
    delivery(tmp.path(), &home, "prime", "post", payload)
        .assert()
        .failure()
        .code(1)
        .stderr(contains("missing binding.operating"));
}

#[test]
fn a_full_claim_work_close_lifecycle_delivers_then_tears_down() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();

    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", inv, "bl-x");

    // claim.post — materialize the code worktree (id off the sealed metadata).
    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "Add feature")).assert().success();
    assert!(wt.join("seed.txt").exists());

    // work happens in the code worktree.
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();

    // close.pre — id recovered from the change worktree's deleted task file.
    let change = tmp.path().join("change");
    fs::create_dir(&change).unwrap();
    git(&change, &["init", "-q", "-b", "balls"]);
    git(&change, &["config", "user.name", "test"]);
    git(&change, &["config", "user.email", "test@example.com"]);
    fs::create_dir(change.join("tasks")).unwrap();
    fs::write(change.join("tasks/bl-x.md"), "x\n").unwrap();
    git(&change, &["add", "-A"]);
    git(&change, &["commit", "-qm", "seed"]);
    fs::remove_file(change.join("tasks/bl-x.md")).unwrap();
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature")).assert().success();

    assert_eq!(
        String::from_utf8(Command::new("git").current_dir(&root).args(["log", "-1", "--format=%s", "main"]).output().unwrap().stdout)
            .unwrap()
            .trim(),
        "Add feature [bl-x]"
    );

    // close.post — teardown removes the worktree.
    delivery(&root, &home, "close", "post", &post(inv, "bl-x", "Add feature")).assert().success();
    assert!(!wt.exists());
}

#[test]
fn missing_op_and_phase_is_a_usage_error() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("bl-delivery")
        .unwrap()
        .env("BALLS_PLUGIN_NAME", "delivery")
        .env("HOME", tmp.path())
        .write_stdin(String::new())
        .assert()
        .failure()
        .code(1)
        .stderr(contains("usage: bl-delivery"));
}

#[test]
fn malformed_stdin_is_an_error() {
    let tmp = TempDir::new().unwrap();
    delivery(tmp.path(), tmp.path(), "claim", "post", "not json").assert().failure().code(1);
}

#[test]
fn a_missing_protocol_env_var_is_reported() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("bl-delivery")
        .unwrap()
        .env_remove("BALLS_PLUGIN_NAME")
        .env("HOME", tmp.path())
        .args(["claim", "post"])
        .write_stdin(post("/proj", "bl-x", "T"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("BALLS_PLUGIN_NAME is unset"));
}
