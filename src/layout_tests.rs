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
