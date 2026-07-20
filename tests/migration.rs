//! bl-868d end-to-end: a fresh clone of a hub whose `origin/balls/tasks` is
//! still the PRE-greenfield LEGACY store (`.balls/tasks/*.json`, no `tasks/`).
//! Before the fix, `bl prime` ADOPTED that branch as the greenfield store (the
//! §12 adopt rule firing on a non-store tip), the delivery plugin aborted on
//! the missing `tasks/`, and every re-prime hit the same abort — the §12
//! no-op-converge property was lost and the §16 runbook's step 2 ("prime
//! founds substrate + an empty store; import fills it") was impossible on any
//! shared repo. Now the tracker QUARANTINES a no-`tasks/` tip (warns, adopts
//! nothing, never rewrites it), so prime founds fresh and converges, and the
//! whole §16 sequence — prime → preview → import → cutover — runs as written.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Git;
use tempfile::TempDir;

/// Run a setup git command, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    assert!(Git::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success());
}

/// `cwd`'s tip of `rev`, trimmed.
fn tip(cwd: &Path, rev: &str) -> String {
    let out = Git::new("git").arg("-C").arg(cwd).args(["rev-parse", rev]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Is `anc` an ancestor of `desc` in `cwd`?
fn is_ancestor(cwd: &Path, anc: &str, desc: &str) -> bool {
    Git::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["merge-base", "--is-ancestor", anc, desc])
        .status()
        .unwrap()
        .success()
}

/// The freshly-built `bl` rooted in `project`, XDG-isolated under the tempdir.
/// Inherited plugin-chain env is scrubbed so a test running inside the close
/// hook can't leak a depth/name into the child (parallel-test isolation).
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

/// A bare hub carrying `main` plus a LEGACY `balls/tasks` (task JSON under
/// `.balls/tasks/`, NO `tasks/`), and a fresh clone of it — the §16 shared-repo
/// migration starting point.
fn legacy_hub_and_clone(tmp: &Path) -> (PathBuf, PathBuf) {
    let hub = tmp.join("hub.git");
    git(tmp, &["init", "--bare", "-q", "-b", "main", &hub.to_string_lossy()]);
    let seed = tmp.join("seed");
    git(tmp, &["clone", "-q", &hub.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "t"]);
    git(&seed, &["config", "user.email", "t@e"]);
    fs::write(seed.join("README.md"), "hi\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "init"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    git(&seed, &["checkout", "-q", "--orphan", "balls/tasks"]);
    git(&seed, &["rm", "-rq", "--cached", "."]);
    fs::remove_file(seed.join("README.md")).unwrap();
    fs::create_dir_all(seed.join(".balls/tasks")).unwrap();
    fs::write(
        seed.join(".balls/tasks/bl-aaaa.json"),
        r#"{"id":"bl-aaaa","title":"legacy task","status":"open","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","description":"carried over"}"#,
    )
    .unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "legacy store"]);
    git(&seed, &["push", "-q", "origin", "balls/tasks"]);
    let clone = tmp.join("clone");
    git(tmp, &["clone", "-q", &hub.to_string_lossy(), &clone.to_string_lossy()]);
    (hub, clone)
}

#[test]
fn prime_on_a_legacy_carrying_hub_founds_fresh_imports_and_cuts_over_fast_forward() {
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let (hub, clone) = legacy_hub_and_clone(tmp.path());
    let legacy_tip = tip(&hub, "balls/tasks");

    // §16 runbook step 2 on a fresh clone: prime succeeds, warns about the
    // un-cut-over legacy ref, and founds a FRESH greenfield store instead of
    // adopting the legacy branch (the bl-868d wedge).
    bl(&clone, &home, &state)
        .arg("prime")
        .assert()
        .success()
        .stderr(contains("not a greenfield store"));
    // §12 no-op-converge: a re-prime succeeds too (it used to re-abort).
    bl(&clone, &home, &state).arg("prime").assert().success();
    // The hub's legacy ref was never rewritten — cutover is the operator's
    // explicit history join + fast-forward push (runbook step 5), not an
    // implicit side effect.
    assert_eq!(tip(&hub, "balls/tasks"), legacy_tip);

    // Steps 3+4: the preview reads the legacy store from the clone's
    // remote-tracking ref, and the cutover button imports it into the fresh
    // store — the per-op publish skips the un-cut-over ref instead of E5ing.
    bl(&clone, &home, &state)
        .args(["list", "--legacy=origin/balls/tasks"])
        .assert()
        .success()
        .stdout(contains("bl-aaaa"));
    bl(&clone, &home, &state)
        .args(["import", "--legacy=origin/balls/tasks", "--as", "mig"])
        .assert()
        .success();
    bl(&clone, &home, &state).arg("list").assert().success().stdout(contains("legacy task"));

    // Step 5: the cutover JOIN (bl-8660) — from the XDG store checkout, merge
    // the legacy tip (`-s ours`: greenfield tree byte-for-byte, merge parented
    // on the legacy tip), then publish with a PLAIN push. The push succeeding
    // without `--force` IS the claim under test: the cutover rewrites nothing.
    let clones = state.join("balls/clones");
    let store = fs::read_dir(&clones).unwrap().next().unwrap().unwrap().path().join("tasks");
    git(&store, &["config", "user.name", "t"]);
    git(&store, &["config", "user.email", "t@e"]);
    let hub_url = hub.to_string_lossy();
    git(&store, &["fetch", "-q", &hub_url, "refs/heads/balls/tasks"]);
    git(&store, &["merge", "-q", "-s", "ours", "--allow-unrelated-histories", "FETCH_HEAD", "-m", "cutover"]);
    git(&store, &["push", "-q", &hub_url, "balls/tasks:refs/heads/balls/tasks"]);
    // The hub's new tip DESCENDS from the legacy tip — every clone of the hub
    // fast-forwards on its next fetch, and the legacy history (closed tasks
    // included) stays readable in-branch at the merge's legacy parent.
    let cut_tip = tip(&hub, "balls/tasks");
    assert!(is_ancestor(&hub, &legacy_tip, &cut_tip));

    // The migration window is CLOSED: the next op's sync/publish resumes as on
    // any federated checkout — no quarantine warning, the hub advances, and
    // the branch is still one fast-forward lineage from the legacy tip.
    bl(&clone, &home, &state)
        .args(["create", "post-cutover", "--as", "mig"])
        .assert()
        .success()
        .stderr(contains("not a greenfield store").not());
    let after = tip(&hub, "balls/tasks");
    assert_ne!(after, cut_tip);
    assert!(is_ancestor(&hub, &legacy_tip, &after));
}

/// One legacy task JSON: the §16-mapped fields plus a per-task `extra` tail
/// (`,"type":"epic"`, `,"parent":…`, `,"depends_on":[…]`) that drives one
/// projection edge. Title is a constant so `epic`/`deferred`/an id only ever
/// appear where the projection put them.
fn legacy(id: &str, status: &str, extra: &str) -> String {
    format!(
        r#"{{"id":"{id}","title":"sample","status":"{status}","created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-02T03:04:06Z"{extra}}}"#
    )
}

/// A repo on `main` plus a `legacy` orphan branch carrying an ENRICHED
/// pre-greenfield store (`.balls/tasks/*.json`) that exercises every §16
/// field-projection edge at once: a closed task (skipped), an epic + its child
/// (implied tag + the import-minted reciprocal edge), a deferred task (implied
/// tag), an orphan whose parent is the closed task (dangling → nulled), and a
/// two-hop `depends_on` chain (claim-blocker mint). Returns the project path.
fn enriched_legacy_repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("proj");
    git(tmp, &["init", "-q", "-b", "main", &repo.to_string_lossy()]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@e"]);
    fs::write(repo.join("README.md"), "hi\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    git(&repo, &["checkout", "-q", "--orphan", "legacy"]);
    git(&repo, &["rm", "-rq", "--cached", "."]);
    fs::remove_file(repo.join("README.md")).unwrap();
    let dir = repo.join(".balls/tasks");
    fs::create_dir_all(&dir).unwrap();
    let fixture = [
        ("bl-clsd", legacy("bl-clsd", "closed", "")),
        ("bl-epic", legacy("bl-epic", "open", r#","type":"epic""#)),
        ("bl-kid", legacy("bl-kid", "open", r#","parent":"bl-epic""#)),
        ("bl-defr", legacy("bl-defr", "deferred", "")),
        ("bl-orph", legacy("bl-orph", "open", r#","parent":"bl-clsd""#)),
        ("bl-dep", legacy("bl-dep", "open", r#","depends_on":["bl-mid"]"#)),
        ("bl-mid", legacy("bl-mid", "open", r#","depends_on":["bl-defr"]"#)),
    ];
    for (id, json) in fixture {
        fs::write(dir.join(format!("{id}.json")), json).unwrap();
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "legacy store"]);
    git(&repo, &["checkout", "-q", "main"]);
    repo
}

#[test]
fn legacy_import_projects_every_field_edge_into_the_greenfield_set() {
    // bl-2f0c: one enriched fixture drives all of §16's per-task field map.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let repo = enriched_legacy_repo(tmp.path());
    let spec = "--legacy=legacy:.balls/tasks";
    bl(&repo, &home, &state).arg("prime").assert().success();

    // The preview (`list --legacy`) is the pure projection: the closed task is
    // already gone (file-absent = resolved, §9), every live ball is present.
    bl(&repo, &home, &state).args(["list", spec]).assert().success().stdout(
        contains("bl-epic")
            .and(contains("bl-kid"))
            .and(contains("bl-defr"))
            .and(contains("bl-orph"))
            .and(contains("bl-dep"))
            .and(contains("bl-mid"))
            .and(contains("bl-clsd").not()),
    );

    // The cutover button imports the six live balls AND wires the epic
    // reciprocal edges through the ordinary `update --needs` machinery (§16).
    bl(&repo, &home, &state)
        .args(["import", spec, "--as", "mig"])
        .assert()
        .success()
        .stderr(contains("import 6 balls"));

    // Closed → absent: the greenfield store never held it, and a `--legacy` miss
    // does not fall through to history — it is a clean "no such ball".
    bl(&repo, &home, &state)
        .args(["show", "bl-clsd", "--json"])
        .assert()
        .failure()
        .stderr(contains("no such ball: bl-clsd"));

    // The epic: `type:epic` synthesized the `epic` tag, and import minted the
    // reciprocal "epic waits on its child" claim-blocker on the parent.
    bl(&repo, &home, &state)
        .args(["show", "bl-epic", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""epic""#).and(contains(r#""id": "bl-kid""#)).and(contains(r#""on": "claim""#)));
    // The child keeps its containment pointer (a live parent is not delinked).
    bl(&repo, &home, &state)
        .args(["show", "bl-kid", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""parent": "bl-epic""#));

    // `status:deferred` → the `deferred` tag.
    bl(&repo, &home, &state)
        .args(["show", "bl-defr", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""deferred""#));

    // Dangling parent (points at the skipped closed task) → nulled, and no
    // stray reciprocal edge was minted for the absent parent.
    bl(&repo, &home, &state)
        .args(["show", "bl-orph", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""parent": null"#).and(contains("bl-clsd").not()));

    // The `depends_on` chain: each hop is a claim-blocker on the dependent ball.
    bl(&repo, &home, &state)
        .args(["show", "bl-dep", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""id": "bl-mid""#).and(contains(r#""on": "claim""#)));
    bl(&repo, &home, &state)
        .args(["show", "bl-mid", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""id": "bl-defr""#));
}

#[test]
fn import_legacy_without_a_legacy_store_refuses_cleanly() {
    // bl-3ddb: no legacy ref here — the refusal names the spec instead of
    // dying on git's raw `fatal: Not a valid object name balls/tasks`.
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));
    let repo = tmp.path().join("repo");
    git(tmp.path(), &["init", "-q", &repo.to_string_lossy()]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@e"]);
    fs::write(repo.join("README.md"), "hi\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    bl(&repo, &home, &state).arg("prime").assert().success();
    bl(&repo, &home, &state)
        .args(["import", "--legacy", "--as", "mig"])
        .assert()
        .failure()
        .stderr(contains("no legacy store at `balls/tasks:.balls/tasks`"));
}
