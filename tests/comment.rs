//! End-to-end proof of the ONE claim that justifies `bl comment` (bl-d136): the
//! note lands in the BODY, which is stored state, so it renders in the human `bl
//! show` AND in the bedrock `bl show --json` — where the derived `-m` journal
//! cannot follow it. Runs the freshly-built `bl` against a throwaway temp project
//! (a real git repo on `main`), never the dev repo's own task list.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so its clone bundle never lands in the real `$HOME`.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project).env("HOME", home).env("XDG_STATE_HOME", state).env_remove("XDG_CONFIG_HOME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project repo on `main` with a seed commit, plus a primed checkout.
fn primed_project(tmp: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

#[test]
fn a_comment_renders_in_both_the_human_and_the_bedrock_view() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let id = created_id(
        bl(&project, &home, &state)
            .args(["create", "A ball", "--body", "the original body", "--as", "me"])
            .assert()
            .success(),
    );

    bl(&project, &home, &state).args(["comment", &id, "the appended note", "--as", "me"]).assert().success();

    // Human view: the body (with the rule seam), plus the derived journal that
    // records the op — the projection that carries everything.
    bl(&project, &home, &state)
        .args(["show", &id])
        .assert()
        .success()
        .stdout(contains("the original body"))
        .stdout(contains("---"))
        .stdout(contains("the appended note"))
        .stdout(contains("journal"))
        .stdout(contains("comment"));

    // Bedrock view: the body carries the comment; the derived journal is absent.
    // This is the whole justification for the verb — a `-m` note would render in
    // the human view above and vanish here.
    let json = bl(&project, &home, &state).args(["show", &id, "--json"]).assert().success();
    let out = String::from_utf8(json.get_output().stdout.clone()).unwrap();
    let record: serde_json::Value = serde_json::from_str(&out).unwrap();
    let body = record["body"].as_str().unwrap();
    assert_eq!(body, "the original body\n\n---\n\nthe appended note\n", "the comment is stored body");
    assert!(record.get("journal").is_none(), "the derived journal never rides the bedrock export");
}

#[test]
fn comment_refuses_empty_text_and_takes_no_note_flag() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());
    let id = created_id(bl(&project, &home, &state).args(["create", "A ball", "--as", "me"]).assert().success());

    // A no-op append would seal nothing — the silent-note-loss failure (bl-cf93).
    bl(&project, &home, &state)
        .args(["comment", &id, "   ", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("TEXT is empty"));

    // `-m` would store the same fact twice.
    bl(&project, &home, &state)
        .args(["comment", &id, "a note", "-m", "same words", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("takes only <id>"));

    // And the ball is untouched by either refusal.
    bl(&project, &home, &state).args(["show", &id]).assert().success().stdout(contains("a note").not());
}
