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
    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str, marker: &str) -> io::Result<()> {
        self.log(format!("deliver {} {branch} -> {integration} : {subject} : {marker}", path.display()));
        Ok(())
    }
    fn is_git_repo(&self) -> io::Result<bool> {
        unreachable!("dispatch never gates on the precondition (see delivery_precondition)")
    }
}

fn spec() -> Spec<'static> {
    Spec {
        worktree: Path::new("/wt"),
        branch: "work/bl-f813",
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
fn prime_post_re_materializes_like_a_claim() {
    // The binary drives one `prime`/`post` per still-claimed ball; each runs the
    // same `materialize` act a `claim` would (§11/§12).
    assert_eq!(drive("prime", "post", false), ["materialize /wt work/bl-f813"]);
}

#[test]
fn unclaim_post_releases() {
    assert_eq!(drive("unclaim", "post", false), ["release /wt"]);
}

#[test]
fn close_pre_resolves_integration_then_delivers() {
    // The marker rides along so deliver can skip a delivery that already landed
    // in an earlier aborted close (retry-idempotence, bl-430e).
    assert_eq!(
        drive("close", "pre", false),
        ["integration", "deliver /wt work/bl-f813 -> main : Title [bl-f813] : [bl-f813]"]
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
fn declining_rollbacks_and_unwired_hooks_are_noops() {
    // close.pre rollback DECLINES (§14, bl-c231): the squash is the BINDING
    // commit point — it stands through an abort and the retried close
    // converges onto it; un-squash is gone (it raced concurrent integration
    // movement). The repo is never even consulted.
    assert!(drive("close", "pre", true).is_empty());
    assert!(drive("close", "post", true).is_empty()); // teardown re-creatable
    assert!(drive("unclaim", "post", true).is_empty()); // release re-creatable
    assert!(drive("create", "post", false).is_empty()); // not our hook
    assert!(drive("claim", "pre", false).is_empty()); // wrong phase
}


#[test]
fn an_integration_failure_aborts_a_close() {
    let repo = FakeRepo { fail_integration: true, ..FakeRepo::default() };
    assert!(dispatch("close", "pre", false, &repo, &spec()).is_err());
}

#[test]
fn worktree_path_mirrors_the_invocation_path_for_a_cargo_safe_dir() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    let p = worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-f813");
    // The code worktree MIRRORS the invocation path (no percent-encoding): a `%`
    // ancestor breaks `rust-lld`'s output paths (bl-f3e4). The leading `/` is
    // stripped so it nests under the territory; the result has no `%`.
    assert_eq!(
        p,
        Path::new("/st/balls/plugins/delivery/home/me/dev/proj/bl-f813")
    );
    assert!(!p.to_string_lossy().contains('%'));
}

#[test]
fn work_branch_is_the_branch_half_of_the_worktree_pair() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    // The branch and path derive from the same `<id>` key through the one pair
    // of helpers — the convergence §11 claimant-keying will edit in one place.
    assert_eq!(work_branch("bl-f813"), "work/bl-f813");
    let p = worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-f813");
    assert_eq!(work_branch("bl-f813"), format!("work/{}", p.file_name().unwrap().to_str().unwrap()));
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
fn claim_and_prime_post_surface_the_bare_path() {
    // The verb's one product, the way `create` prints the id (§11) — printed
    // whether or not the dir pre-existed (claim.post just materialized it).
    let wt = Path::new("/wt/bl-x");
    assert_eq!(surfaced("claim", "post", false, wt, true).as_deref(), Some("/wt/bl-x"));
    assert_eq!(surfaced("prime", "post", false, wt, true).as_deref(), Some("/wt/bl-x"));
}

#[test]
fn show_read_surfaces_a_field_line_only_when_the_worktree_exists() {
    // The §6 read dispatch folds this into `bl show`'s human field block; an
    // absent worktree (released, or claimed on another machine) prints nothing —
    // the plugin asserts nothing git doesn't know (§11).
    let wt = Path::new("/wt/bl-x");
    assert_eq!(surfaced("show", "read", false, wt, true).as_deref(), Some("  worktree /wt/bl-x"));
    assert_eq!(surfaced("show", "read", false, wt, false), None);
}

#[test]
fn no_other_hook_or_rollback_surfaces_anything() {
    // Nothing is ever staged or stored (bl-0af4): every non-surfacing hook —
    // and any rollback — prints nothing.
    for (op, phase, rb, exists) in [
        ("claim", "post", true, true), // a rolled-back claim is not a product
        ("claim", "pre", false, true),
        ("unclaim", "post", false, true),
        ("close", "pre", false, true),
        ("show", "read", true, true), // a read has nothing to roll back, but stay strict
    ] {
        assert_eq!(surfaced(op, phase, rb, Path::new("/wt"), exists), None, "{op}.{phase} rb={rb}");
    }
}

#[test]
fn protocol_self_description_lists_every_hooked_op() {
    let v: serde_json::Value = serde_json::from_str(PROTOCOL_JSON).unwrap();
    assert_eq!(v["protocol"], serde_json::json!([1]));
    assert_eq!(v["ops"], serde_json::json!(["claim", "unclaim", "close", "prime", "show"]));
}

#[test]
fn binding_territory_is_the_parent_of_every_worktree() {
    let xdg = Xdg::with(Path::new("/home/me"), None, Some("/st"));
    let territory = binding_territory(&xdg, "delivery", "/home/me/dev/proj");
    assert_eq!(territory, worktree_path(&xdg, "delivery", "/home/me/dev/proj", "bl-x").parent().unwrap());
}

#[test]
fn wire_deserializes_the_slice_the_plugin_needs() {
    let json = r#"{
        "protocol": 1, "op": "close", "phase": "post", "plugin_name": "delivery",
        "actor": "me", "binding": {"branch": "balls", "store": "/s", "invocation_path": "/proj"},
        "command": {"op": "close"},
        "current_state": {"title": "Refactor foo", "created": 0, "updated": 0},
        "metadata": {"bl-id": ["bl-f813"]}, "commit": "c", "previous_commit": "p"
    }"#;
    let wire: Wire = serde_json::from_str(json).unwrap();
    assert_eq!(wire.actor, "me");
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
    assert_eq!(wire.actor, ""); // absent actor defaults empty (per-ball ops ignore it)
    assert!(wire.metadata.is_none());
    assert!(wire.current_state.is_none());
}

#[test]
fn ensure_safe_invocation_path_admits_clean_absolute_paths() {
    assert!(ensure_safe_invocation_path("/home/mark/dev/balls").is_ok());
    // A literal `..`-prefixed filename (no separator) is fine — it cannot escape.
    assert!(ensure_safe_invocation_path("/home/mark/..foo").is_ok());
}

#[test]
fn ensure_safe_invocation_path_rejects_relative_and_dotdot() {
    assert!(ensure_safe_invocation_path("home/mark/dev").is_err()); // not absolute
    assert!(ensure_safe_invocation_path("/home/../../etc").is_err()); // `..` traversal
}
