//! End-to-end coverage for the §16 legacy→greenfield migrator
//! (`scripts/migrate-legacy.py`). The crate's 100% line-coverage gate measures
//! only the Rust library; this 400-line Python transform is the largest piece of
//! shipped behavior OUTSIDE that gate, and its in-script `--self-test` exercises
//! only the pure transform (`fold_notes` is stubbed there). This harness closes
//! the honesty gap: it (1) runs that self-test under `cargo test` so a transform
//! regression can no longer rot silently, and (2) drives the git-touching half —
//! `load_legacy` / `fold_notes` / `guard` / `build_refs` / `subtree` — against a
//! throwaway legacy repo, asserting the produced greenfield `tasks/<id>.md`.

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/migrate-legacy.py")
}

fn git(cwd: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed");
}

/// Run the migrator; return (success, stdout, stderr).
fn migrate(args: &[&str]) -> (bool, String, String) {
    let out = Command::new("python3").arg(script()).args(args).output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `git show <obj>` content, or None if the object is absent (a skipped task).
fn show(cwd: &Path, obj: &str) -> Option<String> {
    let o = Command::new("git").arg("-C").arg(cwd).args(["show", obj]).output().unwrap();
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// A legacy repo: a `balls/tasks` orphan branch holding `.balls/tasks/*` (the
/// JSON store + any notes), with `main` left checked out. `files` are
/// (name, content) pairs written under `.balls/tasks/`.
fn legacy_repo(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let r = tmp.path();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["config", "user.email", "t@e"]);
    git(r, &["config", "user.name", "t"]);
    std::fs::write(r.join("seed"), "s\n").unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "seed"]);
    git(r, &["checkout", "-q", "--orphan", "balls/tasks"]);
    git(r, &["rm", "-rfq", "."]); // clear main's content off the orphan
    let td = r.join(".balls/tasks");
    std::fs::create_dir_all(&td).unwrap();
    for (name, body) in files {
        std::fs::write(td.join(name), body).unwrap();
    }
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "legacy"]);
    git(r, &["checkout", "-q", "main"]);
    tmp
}

fn task(id: &str, ty: &str, status: &str, fields: &str) -> String {
    format!(
        r#"{{"id":"{id}","type":"{ty}","status":"{status}","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"{fields}}}"#
    )
}

#[test]
fn the_scripts_own_self_test_passes() {
    // The pure-transform invariants (closed-skip, epic edge, dangling null,
    // depends_on→blocker, TOML field order). Brought under `cargo test` so it
    // runs in every gate, not only when someone remembers to invoke it by hand.
    let (ok, out, err) = migrate(&["--self-test"]);
    assert!(ok, "self-test failed: {err}");
    assert!(out.contains("self-test: OK"), "{out}");
}

#[test]
fn build_refs_transforms_a_real_legacy_store_end_to_end() {
    // A title with a quote exercises `toml_str` escaping over the git path (the
    // self-test asserts order, not escaping). bl-epic also carries a notes.jsonl
    // so `fold_notes` — STUBBED in the self-test — runs for real here.
    let epic = task("bl-epic", "epic", "open", r#","title":"E \"x\"","priority":1,"tags":[],"depends_on":[],"description":"epic body""#);
    let kid = task("bl-kid", "task", "open", r#","title":"K","parent":"bl-epic","priority":2,"tags":["x"],"depends_on":["bl-dep"],"description":"kid""#);
    let dead = task("bl-dead", "task", "closed", r#","title":"D""#);
    let orphan = task("bl-orphan", "task", "deferred", r#","title":"O","parent":"bl-dead","priority":3,"depends_on":[]"#);
    let files = [
        ("bl-epic.json", epic.as_str()),
        ("bl-epic.notes.jsonl", r#"{"ts":"2026-01-01","author":"a","text":"hello"}"#),
        ("bl-kid.json", kid.as_str()),
        ("bl-dead.json", dead.as_str()),
        ("bl-orphan.json", orphan.as_str()),
    ];
    let repo = legacy_repo(&files);
    let out = TempDir::new().unwrap();
    let cfg = Path::new(env!("CARGO_MANIFEST_DIR")).join("default-config");
    let (ok, stdout, err) = migrate(&[
        "--repo", &repo.path().to_string_lossy(),
        "--config-src", &cfg.to_string_lossy(),
        "--out", &out.path().to_string_lossy(),
        "--build-refs",
    ]);
    assert!(ok, "migrate failed: {err}");
    // The summary counts only LIVE tasks (closed is file-absent = resolved).
    assert!(stdout.contains("4 tasks (3 live) → greenfield: 3 migrated"), "{stdout}");

    let r = repo.path();
    let epic_md = show(r, "refs/migrate/balls-tasks:tasks/bl-epic.md").expect("epic migrated");
    assert!(epic_md.contains(r#"title = "E \"x\"""#), "toml escape: {epic_md}");
    assert!(epic_md.contains(r#"tags = ["epic"]"#), "type=epic → tag: {epic_md}");
    // Epic reciprocal edge: the live child claim-blocks its parent (§10).
    assert!(epic_md.contains("[[blockers]]") && epic_md.contains(r#"id = "bl-kid""#), "epic edge: {epic_md}");
    // fold_notes ran over the git path, not the stub.
    assert!(epic_md.contains("## Notes (migrated)") && epic_md.contains("hello"), "notes fold: {epic_md}");

    let kid_md = show(r, "refs/migrate/balls-tasks:tasks/bl-kid.md").expect("kid migrated");
    assert!(kid_md.contains(r#"parent = "bl-epic""#) && kid_md.contains(r#"tags = ["x"]"#), "{kid_md}");
    assert!(kid_md.contains(r#"id = "bl-dep""#), "depends_on → blocker: {kid_md}");

    let orphan_md = show(r, "refs/migrate/balls-tasks:tasks/bl-orphan.md").expect("orphan migrated");
    assert!(!orphan_md.contains("parent ="), "dangling parent nulled: {orphan_md}");
    assert!(orphan_md.contains(r#"tags = ["deferred"]"#), "deferred → tag: {orphan_md}");

    // Closed task: no file (file-absent = resolved).
    assert!(show(r, "refs/migrate/balls-tasks:tasks/bl-dead.md").is_none(), "closed skipped");

    // Config branch carries the seed verbatim (the migrated config IS the seed).
    let seed = std::fs::read_to_string(cfg.join("balls.toml")).unwrap();
    assert_eq!(show(r, "refs/migrate/balls-config:config/balls.toml").unwrap(), seed);
}

#[test]
fn the_one_shot_guards_refuse_then_force_overrides() {
    // Both guard branches at once: a present `balls/config` (looks already
    // migrated) AND a claimed live task (in-flight work would be stranded).
    let claimed = task("bl-live", "task", "open", r#","title":"L","claimed_by":"someone""#);
    let repo = legacy_repo(&[("bl-live.json", claimed.as_str())]);
    git(repo.path(), &["checkout", "-q", "--orphan", "balls/config"]);
    git(repo.path(), &["rm", "-rfq", "."]);
    std::fs::write(repo.path().join("k"), "k\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-qm", "landing"]);
    git(repo.path(), &["checkout", "-q", "main"]);

    let repo_arg = repo.path().to_string_lossy().into_owned();
    let (ok, _out, err) = migrate(&["--repo", &repo_arg, "--dry-run"]);
    assert!(!ok, "should refuse");
    assert!(err.contains("already migrated"), "config-exists guard: {err}");
    assert!(err.contains("live tasks are claimed"), "claimed guard: {err}");

    // --force overrides both; --dry-run keeps it side-effect-free.
    let (ok, out, err) = migrate(&["--repo", &repo_arg, "--force", "--dry-run"]);
    assert!(ok, "force should pass: {err}");
    assert!(out.contains("dry-run: nothing written"), "{out}");
}
