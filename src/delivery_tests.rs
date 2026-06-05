//! Unit tests for the §11 delivery policy: the [`dispatch`] hook→act matrix
//! against a fake [`Repo`] (every branch without a temp repo), plus the pure
//! path/id/subject helpers.

use super::*;
use std::cell::RefCell;
use std::path::Path;

/// A [`Repo`] that records each act and can be told to fail [`Repo::integration`]
/// — enough to assert which act a hook performs and that an `integration()`
/// error propagates.
#[derive(Default)]
struct FakeRepo {
    calls: RefCell<Vec<String>>,
    fail_integration: bool,
}

impl FakeRepo {
    fn log(&self, call: String) {
        self.calls.borrow_mut().push(call);
    }
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl Repo for FakeRepo {
    fn materialize(&self, path: &Path, branch: &str) -> io::Result<()> {
        self.log(format!("materialize {} {branch}", path.display()));
        Ok(())
    }
    fn release(&self, path: &Path) -> io::Result<()> {
        self.log(format!("release {}", path.display()));
        Ok(())
    }
    fn discard(&self, path: &Path, branch: &str) -> io::Result<()> {
        self.log(format!("discard {} {branch}", path.display()));
        Ok(())
    }
    fn integration(&self) -> io::Result<String> {
        if self.fail_integration {
            return Err(io::Error::other("no integration branch"));
        }
        self.log("integration".into());
        Ok("main".into())
    }
    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str) -> io::Result<()> {
        self.log(format!("deliver {} {branch} -> {integration} : {subject}", path.display()));
        Ok(())
    }
    fn unsquash(&self, integration: &str, marker: &str) -> io::Result<()> {
        self.log(format!("unsquash {integration} {marker}"));
        Ok(())
    }
}

fn spec() -> Spec<'static> {
    Spec {
        worktree: Path::new("/wt"),
        branch: "work/bl-f813",
        cwd: None,
        subject: "Title [bl-f813]",
        marker: "[bl-f813]",
    }
}

/// Drive one hook against a fresh fake and return the calls it made.
fn drive(op: &str, phase: &str, rolling_back: bool) -> Vec<String> {
    let repo = FakeRepo::default();
    dispatch(op, phase, rolling_back, &repo, &spec()).unwrap();
    repo.calls()
}

#[test]
fn claim_post_materializes() {
    assert_eq!(drive("claim", "post", false), ["materialize /wt work/bl-f813"]);
}

#[test]
fn unclaim_and_drop_post_release() {
    assert_eq!(drive("unclaim", "post", false), ["release /wt"]);
    assert_eq!(drive("drop", "post", false), ["release /wt"]);
}

#[test]
fn close_pre_resolves_integration_then_delivers() {
    assert_eq!(
        drive("close", "pre", false),
        ["integration", "deliver /wt work/bl-f813 -> main : Title [bl-f813]"]
    );
}

#[test]
fn close_post_releases() {
    assert_eq!(drive("close", "post", false), ["release /wt"]);
}

#[test]
fn claim_post_rollback_discards_worktree_and_branch() {
    assert_eq!(drive("claim", "post", true), ["discard /wt work/bl-f813"]);
}

#[test]
fn close_pre_rollback_unsquashes_when_marked() {
    assert_eq!(drive("close", "pre", true), ["integration", "unsquash main [bl-f813]"]);
}

#[test]
fn re_creatable_rollbacks_and_unwired_hooks_are_noops() {
    assert!(drive("close", "post", true).is_empty()); // teardown re-creatable
    assert!(drive("unclaim", "post", true).is_empty()); // release re-creatable
    assert!(drive("drop", "post", true).is_empty());
    assert!(drive("create", "post", false).is_empty()); // not our hook
    assert!(drive("claim", "pre", false).is_empty()); // wrong phase
}

#[test]
fn close_hooks_refuse_when_invoked_from_inside_the_worktree() {
    let repo = FakeRepo::default();
    let inside = Spec { cwd: Some(Path::new("/wt/sub")), ..spec() };
    for phase in ["pre", "post"] {
        let err = dispatch("close", phase, false, &repo, &inside).unwrap_err();
        assert!(err.to_string().contains("cwd is inside the worktree"));
    }
    assert!(repo.calls().is_empty()); // refused before any git act
}

#[test]
fn an_equal_cwd_refuses_but_an_unknown_one_waves_through() {
    let repo = FakeRepo::default();
    let at = Spec { cwd: Some(Path::new("/wt")), ..spec() };
    assert!(dispatch("close", "post", false, &repo, &at).is_err());
    // `$PWD` unset (cwd: None) → best-effort guard allows the teardown.
    assert_eq!(drive("close", "post", false), ["release /wt"]);
}

#[test]
fn an_integration_failure_aborts_a_close() {
    let repo = FakeRepo { fail_integration: true, ..FakeRepo::default() };
    assert!(dispatch("close", "pre", false, &repo, &spec()).is_err());
    assert!(dispatch("close", "pre", true, &repo, &spec()).is_err());
}

#[test]
fn worktree_path_is_the_derived_xdg_formula() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    let p = worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-f813");
    assert_eq!(
        p,
        Path::new("/st/balls/plugins/delivery/%2Fhome%2Fme%2Fdev%2Fproj/bl-f813")
    );
}

#[test]
fn subject_and_marker_carry_the_delivery_tag() {
    assert_eq!(subject("Refactor foo", "bl-f813"), "Refactor foo [bl-f813]");
    assert_eq!(marker("bl-f813"), "[bl-f813]");
}

#[test]
fn resolve_id_prefers_the_sealed_metadata_trailer() {
    let mut md = Metadata::new();
    md.insert("bl-id".into(), vec!["bl-abc1".into()]);
    let id = resolve_id(Some(&md), || unreachable!("git is not consulted when metadata carries the id")).unwrap();
    assert_eq!(id, "bl-abc1");
}

#[test]
fn resolve_id_reads_the_single_changed_task_file_on_a_pre_hook() {
    let id = resolve_id(None, || Ok(vec!["tasks/bl-9f9f.md".into(), "README.md".into()])).unwrap();
    assert_eq!(id, "bl-9f9f");
}

#[test]
fn resolve_id_rejects_zero_or_many_changed_task_files() {
    let none = resolve_id(None, || Ok(vec!["README.md".into()])).unwrap_err();
    assert!(none.to_string().contains("found 0"));
    let many = resolve_id(None, || Ok(vec!["tasks/a.md".into(), "tasks/b.md".into()])).unwrap_err();
    assert!(many.to_string().contains("found 2"));
}

#[test]
fn resolve_id_propagates_a_lister_error() {
    let err = resolve_id(None, || Err(io::Error::other("git blew up"))).unwrap_err();
    assert_eq!(err.to_string(), "git blew up");
}

#[test]
fn protocol_self_description_lists_the_four_hooked_ops() {
    let v: serde_json::Value = serde_json::from_str(PROTOCOL_JSON).unwrap();
    assert_eq!(v["protocol"], serde_json::json!([1]));
    assert_eq!(v["ops"], serde_json::json!(["claim", "unclaim", "drop", "close"]));
}

#[test]
fn wire_deserializes_the_slice_the_plugin_needs() {
    let json = r#"{
        "protocol": 1, "op": "close", "phase": "post", "plugin_name": "delivery",
        "actor": "me", "binding": {"branch": "balls", "operating": "/o", "invocation_path": "/proj"},
        "command": {"op": "close"},
        "current_state": {"title": "Refactor foo", "created": 0, "updated": 0},
        "metadata": {"bl-id": ["bl-f813"]}, "commit": "c", "previous_commit": "p"
    }"#;
    let wire: Wire = serde_json::from_str(json).unwrap();
    assert_eq!(wire.binding.invocation_path, "/proj");
    assert_eq!(wire.current_state.unwrap().title, "Refactor foo");
    assert_eq!(wire.metadata.unwrap()["bl-id"], ["bl-f813"]);
    assert!(wire.rolling_back.is_none());
}

#[test]
fn wire_tolerates_a_minimal_pre_payload_and_a_rollback_tag() {
    let json = r#"{"binding": {"invocation_path": "/p"}, "rolling_back": "pre"}"#;
    let wire: Wire = serde_json::from_str(json).unwrap();
    assert_eq!(wire.rolling_back.as_deref(), Some("pre"));
    assert!(wire.metadata.is_none());
    assert!(wire.current_state.is_none());
}
