use super::*;

fn home() -> &'static Path {
    Path::new("/home/mark")
}

#[test]
fn xdg_variables_when_set_override_the_home_defaults() {
    let x = Xdg::with(home(), Some("/cfg"), Some("/st"));
    assert_eq!(x.user_config(), Path::new("/cfg/balls/config.toml"));
    assert_eq!(x.state_dir(), Path::new("/st/balls"));
}

#[test]
fn absent_or_empty_variables_fall_back_under_home() {
    // `None` and `Some("")` both take the default branch.
    let x = Xdg::with(home(), None, Some(""));
    assert_eq!(x.user_config(), Path::new("/home/mark/.config/balls/config.toml"));
    assert_eq!(x.state_dir(), Path::new("/home/mark/.local/state/balls"));
}

#[test]
fn a_plugin_gets_a_territory_root_under_state() {
    let x = Xdg::with(home(), None, None);
    assert_eq!(
        x.plugin_territory("tracker"),
        Path::new("/home/mark/.local/state/balls/plugins/tracker")
    );
}

#[test]
fn the_clone_bundle_encodes_the_invocation_path_to_one_component() {
    let x = Xdg::with(home(), None, Some("/st"));
    let c = x.clone_dir(Path::new("/home/mark/dev/balls"));
    assert_eq!(
        c.root(),
        Path::new("/st/balls/clones/%2Fhome%2Fmark%2Fdev%2Fballs")
    );
    // Every bundle sits directly under the shared `clones/` parent (the fleet
    // view enumerates it).
    assert_eq!(x.clones_dir(), Path::new("/st/balls/clones"));
    assert_eq!(c.root().parent(), Some(x.clones_dir().as_path()));
}

#[test]
fn the_bundle_names_its_inhabitants() {
    let c = Xdg::with(home(), None, Some("/st")).clone_dir(Path::new("/p"));
    let root = c.root().to_path_buf();
    assert_eq!(c.binding(), root.join("binding.toml"));
    assert_eq!(c.landing(), root.join("config"));
    assert_eq!(c.store(), root.join("tasks"));
    assert_eq!(c.change("abc-123"), root.join("changes/abc-123"));
    assert_eq!(c.op_log(), root.join("log"));
}

/// Found (fake) a clone dir's landing for `path` under `x` — just enough of
/// [`crate::substrate::found_landing`]'s shape (a `config/` dir under the
/// percent-encoded clone bundle) for [`Xdg::nearest_founded_ancestor`]'s own
/// predicate to see it, with no git repo needed.
fn found(x: &Xdg, path: &Path) {
    std::fs::create_dir_all(x.clone_dir(path).landing().join("config")).unwrap();
}

#[test]
fn no_founded_ancestor_is_none_the_common_case() {
    let tmp = tempfile::TempDir::new().unwrap();
    let x = Xdg::with(tmp.path(), None, Some(&tmp.path().join("st").to_string_lossy()));
    let deep = tmp.path().join("a/b/c");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(x.nearest_founded_ancestor(&deep), None);
}

#[test]
fn a_founded_parent_is_found_by_walking_up() {
    let tmp = tempfile::TempDir::new().unwrap();
    let x = Xdg::with(tmp.path(), None, Some(&tmp.path().join("st").to_string_lossy()));
    let project = tmp.path().join("proj");
    let sub = project.join("src");
    std::fs::create_dir_all(&sub).unwrap();
    found(&x, &project);
    assert_eq!(x.nearest_founded_ancestor(&sub), Some(project));
}

#[test]
fn the_nearest_founded_ancestor_wins_over_a_further_one() {
    let tmp = tempfile::TempDir::new().unwrap();
    let x = Xdg::with(tmp.path(), None, Some(&tmp.path().join("st").to_string_lossy()));
    let outer = tmp.path().join("outer");
    let inner = outer.join("inner");
    let sub = inner.join("src");
    std::fs::create_dir_all(&sub).unwrap();
    found(&x, &outer);
    found(&x, &inner);
    assert_eq!(x.nearest_founded_ancestor(&sub), Some(inner), "the closer ancestor wins, not the outer one");
}

#[test]
fn a_founded_store_at_path_itself_is_never_self_reported() {
    // `nearest_founded_ancestor` is only ever called on the miss branch of prime
    // (the caller already knows `path` has no store), but the walk itself must
    // still skip `path` — it starts at the PARENT, never `path` itself.
    let tmp = tempfile::TempDir::new().unwrap();
    let x = Xdg::with(tmp.path(), None, Some(&tmp.path().join("st").to_string_lossy()));
    let project = tmp.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    found(&x, &project);
    assert_eq!(x.nearest_founded_ancestor(&project), None, "path itself is excluded from its own ancestor walk");
}
