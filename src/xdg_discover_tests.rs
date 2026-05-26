use super::*;
use crate::clone_json::CloneJson;
use crate::tracker_json::TrackerJson;
use std::fs;

fn bases(home: &Path) -> XdgBases {
    XdgBases::with(home, None, None, None)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// Touch enough of a tracker checkout for `try_resolve` to recognize
/// it: a `.git` directory and the `.balls/` shape the SPEC promises.
fn fake_tracker_checkout(dir: &Path) {
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::create_dir_all(dir.join(".balls/tasks")).unwrap();
    fs::write(
        dir.join(".balls/project.json"),
        "{\"version\":1,\"id_length\":4,\"plugins\":{}}",
    )
    .unwrap();
    fs::write(dir.join(".balls/repo.json"), "{}").unwrap();
}

#[test]
fn try_resolve_returns_none_without_clone_json_or_tracker_checkout() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    let got = try_resolve(&bases, &clone_root, Some("git@host:owner/repo.git")).unwrap();
    assert!(got.is_none(), "no XDG state → fall through to legacy");
}

#[test]
fn try_resolve_returns_none_without_origin_and_no_stealth() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    let got = try_resolve(&bases, &clone_root, None).unwrap();
    assert!(got.is_none(), "no origin & no stealth → no XDG resolution");
}

#[test]
fn try_resolve_stealth_short_circuits_origin_and_trackers() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    let nested = nested_clone_path(&clone_root);
    let cj_file = clone_json_path(&bases, &nested);
    let tasks = home.path().join("stash/tasks");
    fs::create_dir_all(&tasks).unwrap();
    write_json(
        &cj_file,
        &CloneJson {
            stealth: true,
            tasks_dir: Some(tasks.to_string_lossy().into_owned()),
            ..Default::default()
        },
    );

    // No origin and no tracker checkout — stealth still resolves.
    let got = try_resolve(&bases, &clone_root, None).unwrap().expect("stealth");
    assert!(got.stealth);
    assert_eq!(got.tasks_dir, tasks);
    assert!(got.repo_json_path.is_none(), "stealth: no repo.json");
    assert!(got.project_config_path.is_none(), "stealth: no project.json");
}

#[test]
fn try_resolve_stealth_requires_tasks_dir() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    // Write clone.json by hand so we can produce the invalid
    // "stealth=true without tasks_dir" shape that CloneJson::from_json
    // rejects on read. (CloneJson::save would refuse the same shape.)
    let nested = nested_clone_path(&clone_root);
    let cj_file = clone_json_path(&bases, &nested);
    fs::create_dir_all(cj_file.parent().unwrap()).unwrap();
    fs::write(&cj_file, r#"{"stealth": true}"#).unwrap();

    let err = try_resolve(&bases, &clone_root, None).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("stealth"), "got: {msg}");
    assert!(msg.contains("tasks_dir"), "got: {msg}");
}

#[test]
fn try_resolve_normal_finds_tracker_checkout_by_enc_origin() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("home/u/dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    let origin = "git@github.com:owner/proj.git";
    let enc = percent_encode_component(&canonicalize_origin(origin));
    let own = own_tracker_checkout(&bases, &enc);
    fake_tracker_checkout(&own);

    let got = try_resolve(&bases, &clone_root, Some(origin)).unwrap().expect("xdg");
    assert!(!got.stealth);
    assert_eq!(got.state_repo, own);
    assert_eq!(got.tasks_dir, own.join(".balls/tasks"));
    assert_eq!(got.repo_json_path.as_deref(), Some(own.join(".balls/repo.json").as_path()));
    assert_eq!(got.project_config_path.as_deref(), Some(own.join(".balls/project.json").as_path()));
}

#[test]
fn try_resolve_normal_follows_single_hop_redirect() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("home/u/dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    let own_url = "git@github.com:owner/proj.git";
    let federated_url = "git@github.com:org/tracker.git";

    let own_enc = percent_encode_component(&canonicalize_origin(own_url));
    let own = own_tracker_checkout(&bases, &own_enc);
    fake_tracker_checkout(&own);
    write_json(
        &own.join(".balls/tracker.json"),
        &TrackerJson { state_url: federated_url.into(), state_branch: None },
    );

    let fed_enc = percent_encode_component(&canonicalize_origin(federated_url));
    let fed_branch = percent_encode_component("balls/tasks");
    let fed = tracker_checkout(&bases, &fed_enc, &fed_branch);
    fake_tracker_checkout(&fed);

    let got = try_resolve(&bases, &clone_root, Some(own_url)).unwrap().expect("xdg");
    // tasks come from the federated checkout; repo.json from own.
    assert_eq!(got.state_repo, fed);
    assert_eq!(got.tasks_dir, fed.join(".balls/tasks"));
    assert_eq!(got.repo_json_path.as_deref(), Some(own.join(".balls/repo.json").as_path()));
}

#[test]
fn try_resolve_chained_redirect_aborts() {
    let home = tempfile::TempDir::new().unwrap();
    let clone_root = home.path().join("home/u/dev/proj");
    fs::create_dir_all(&clone_root).unwrap();
    let bases = bases(home.path());

    let own_url = "git@github.com:owner/proj.git";
    let hop1_url = "git@github.com:org/hop1.git";
    let hop2_url = "git@github.com:org/hop2.git";

    let own = own_tracker_checkout(&bases, &percent_encode_component(&canonicalize_origin(own_url)));
    fake_tracker_checkout(&own);
    write_json(
        &own.join(".balls/tracker.json"),
        &TrackerJson { state_url: hop1_url.into(), state_branch: None },
    );

    let hop1 = own_tracker_checkout(&bases, &percent_encode_component(&canonicalize_origin(hop1_url)));
    fake_tracker_checkout(&hop1);
    // Defense-in-depth: even though SPEC §5 forbids tracker.json on a
    // federated tracker, discover must abort if it ever finds one.
    write_json(
        &hop1.join(".balls/tracker.json"),
        &TrackerJson { state_url: hop2_url.into(), state_branch: None },
    );

    let err = try_resolve(&bases, &clone_root, Some(own_url)).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("chained redirect detected"), "got: {msg}");
}

#[test]
fn read_repo_json_returns_default_for_stealth() {
    let home = tempfile::TempDir::new().unwrap();
    let nested = PathBuf::from("dev/p");
    let bases = bases(home.path());
    let res = XdgResolution {
        stealth: true,
        nested_clone: nested.clone(),
        tasks_dir: home.path().join("tasks"),
        state_repo: home.path().join("st"),
        per_clone: PerClonePaths::new(&bases, &nested),
        repo_json_path: None,
        project_config_path: None,
        clone_json_file: home.path().join("cj"),
        clone_json: None,
    };
    let got = read_repo_json(&res).unwrap();
    // The all-defaults shape comes back when stealth → repo_json_path is None.
    assert_eq!(got, RepoJson::default());
}

#[test]
fn build_store_stealth_uses_default_branch() {
    let home = tempfile::TempDir::new().unwrap();
    let nested = PathBuf::from("dev/p");
    let bases = bases(home.path());
    let res = XdgResolution {
        stealth: true,
        nested_clone: nested.clone(),
        tasks_dir: home.path().join("tasks"),
        state_repo: home.path().join("st"),
        per_clone: PerClonePaths::new(&bases, &nested),
        repo_json_path: None,
        project_config_path: None,
        clone_json_file: home.path().join("cj"),
        clone_json: None,
    };
    let store = build_store(home.path().join("clone"), res);
    assert!(store.stealth);
    assert_eq!(store.state_branch(), crate::tracker_address::DEFAULT_BRANCH);
    assert_eq!(store.layout, crate::store::Layout::Xdg);
    // worktrees_root on XDG returns the per-clone field unconditionally —
    // no read-config detour like the legacy branch.
    let wt = store.worktrees_root().unwrap();
    assert!(wt.starts_with(home.path()));
    assert!(wt.ends_with("worktrees/dev/p"));
}

#[test]
fn build_store_non_stealth_defaults_branch_when_state_repo_is_not_a_git_dir() {
    let home = tempfile::TempDir::new().unwrap();
    let nested = PathBuf::from("dev/p");
    let bases = bases(home.path());
    // state_repo points at a directory with no .git — git_current_branch
    // fails, build_store falls back to DEFAULT_BRANCH.
    let st = home.path().join("st");
    fs::create_dir_all(&st).unwrap();
    let res = XdgResolution {
        stealth: false,
        nested_clone: nested.clone(),
        tasks_dir: st.join(".balls/tasks"),
        state_repo: st.clone(),
        per_clone: PerClonePaths::new(&bases, &nested),
        repo_json_path: Some(st.join(".balls/repo.json")),
        project_config_path: Some(st.join(".balls/project.json")),
        clone_json_file: home.path().join("cj"),
        clone_json: None,
    };
    let store = build_store(home.path().join("clone"), res);
    assert!(!store.stealth);
    assert_eq!(store.state_branch(), crate::tracker_address::DEFAULT_BRANCH);
}

#[test]
fn read_repo_json_reads_file_when_present() {
    let dir = tempfile::TempDir::new().unwrap();
    let p = dir.path().join("repo.json");
    fs::write(&p, r#"{"stale_threshold_seconds": 7}"#).unwrap();
    let bases = bases(dir.path());
    let res = XdgResolution {
        stealth: false,
        nested_clone: PathBuf::from("x"),
        tasks_dir: dir.path().join("t"),
        state_repo: dir.path().join("s"),
        per_clone: PerClonePaths::new(&bases, &PathBuf::from("x")),
        repo_json_path: Some(p),
        project_config_path: None,
        clone_json_file: dir.path().join("c"),
        clone_json: None,
    };
    let got = read_repo_json(&res).unwrap();
    assert_eq!(got.stale_threshold_seconds, 7);
}
