//! Integration coverage for bl-f37b: `--resolve-remote` on `bl show`
//! routes through the `delivered_repo` cache when the local tag scan
//! misses, and the JSON contract exposes `delivered_in_resolved_repo`
//! whenever a sha is resolved.

mod common;

use common::*;

#[test]
fn show_default_resolves_local_repo_in_resolved_repo_field() {
    // The bl-f37b JSON contract: `delivered_in_resolved_repo` is
    // always set whenever `delivered_in_resolved` is. With no
    // `--resolve-remote` flag, a local-hit resolution must still
    // populate it from the current clone so downstream tooling can
    // tell which repo the sha came from.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "local-hit");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("f.txt"), "x").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();

    let json_out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    assert!(j["delivered_in_resolved"].is_string());
    let resolved_repo = j["delivered_in_resolved_repo"]
        .as_str()
        .expect("resolved_repo set on local hit");
    // No `origin` in a fresh test repo → falls back to the basename
    // of the repo root (see `repo_url::current`).
    let basename = repo.path().file_name().unwrap().to_string_lossy();
    assert_eq!(resolved_repo, basename);
}

#[test]
fn show_resolve_remote_falls_back_via_delivered_repo() {
    // A bare tracker clone has no code, so the local tag scan misses. With
    // `--resolve-remote` and a fetchable `delivered_repo`, the cross-
    // repo lookup must produce the sha and tag
    // `delivered_in_resolved_repo` with the URL we resolved through.
    let code = new_repo();
    init_in(code.path());
    let id = create_task(code.path(), "x-repo");
    bl_as(code.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = code.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("f.txt"), "x").unwrap();
    bl(code.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();
    bl(code.path())
        .args(["close", &id, "-m", "ok"])
        .assert()
        .success();

    // Reader-side clone: a separate repo whose history does not
    // contain the [bl-id] commit, but the task carries delivered_repo
    // pointing at the code repo. Restore the archived task file (close
    // removed it from the state branch tip) so `bl show` has something
    // to read.
    let reader = new_repo();
    init_in(reader.path());
    let state_parent = git_state(code.path(), &["rev-parse", "balls/tasks~1"])
        .trim()
        .to_string();
    let mut content_json: serde_json::Value = serde_json::from_str(&git_state(
        code.path(),
        &["show", &format!("{state_parent}:.balls/tasks/{id}.json")],
    ))
    .unwrap();
    // Force-set delivered_repo to the source clone's path so the
    // reader has a fetchable URL even though the source's auto-
    // derived value would be the basename (no `origin`).
    content_json["delivered_repo"] =
        serde_json::Value::String(code.path().to_string_lossy().into_owned());
    let id_path = reader
        .path()
        .join(".balls/tasks")
        .join(format!("{id}.json"));
    std::fs::write(&id_path, serde_json::to_string(&content_json).unwrap()).unwrap();

    // Default `bl show` cannot resolve the sha: the local code lacks
    // the commit. The new field is null in this baseline.
    let baseline = bl(reader.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let b: serde_json::Value = serde_json::from_slice(&baseline.stdout).unwrap();
    assert!(b["delivered_in_resolved"].is_null());
    assert!(b["delivered_in_resolved_repo"].is_null());

    // Opting in resolves through the named code repo.
    let out = bl(reader.path())
        .args(["show", &id, "--json", "--resolve-remote"])
        .output()
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let resolved = j["delivered_in_resolved"]
        .as_str()
        .expect("remote resolution produces sha");
    let subject = git(code.path(), &["log", "-1", "--format=%s", resolved]);
    assert!(subject.contains(&format!("[{id}]")));
    assert_eq!(
        j["delivered_in_resolved_repo"].as_str().unwrap(),
        code.path().to_string_lossy(),
    );
}

#[test]
fn show_resolve_remote_unreachable_url_soft_fails() {
    // An unreachable `delivered_repo` must not break `bl show`: the
    // command still exits 0 with `delivered_in_resolved` null and a
    // warning on stderr.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "unreach");

    // Inject a delivered_repo on the task file so the resolver has
    // something to try, but point it at a nonexistent path.
    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{id}.json"));
    let mut j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    j["delivered_repo"] = serde_json::Value::String("/no/such/path/repo.git".into());
    std::fs::write(&path, serde_json::to_string(&j).unwrap()).unwrap();

    let out = bl(repo.path())
        .args(["show", &id, "--json", "--resolve-remote"])
        .output()
        .unwrap();
    assert!(out.status.success(), "soft-fail must keep bl show alive");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["delivered_in_resolved"].is_null());
    assert!(v["delivered_in_resolved_repo"].is_null());
}
