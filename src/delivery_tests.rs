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
    fn mint(&self, branch: &str, base: &str) -> io::Result<()> {
        self.log(format!("mint {branch} at {base}"));
        Ok(())
    }
    fn deliver(&self, path: &Path, branch: &str, integration: &str, message: &str, marker: &str) -> io::Result<()> {
        self.log(format!("deliver {} {branch} -> {integration} : {message} : {marker}", path.display()));
        Ok(())
    }
    fn work_messages(&self, _branch: &str, _integration: &str) -> io::Result<Vec<String>> {
        Ok(Vec::new()) // no authored work → deliver_close falls back to the subject
    }
    fn is_git_repo(&self) -> io::Result<bool> {
        unreachable!("dispatch never gates on the precondition (see delivery_precondition)")
    }
}

fn spec() -> Spec<'static> {
    targeted(None)
}

/// The same spec with a §11 delivery target (bl-7b71) — the id of the ball whose
/// `work/<id>` ref this op forks from and folds back into.
fn targeted(target: Option<&'static str>) -> Spec<'static> {
    Spec {
        worktree: Path::new("/wt"),
        branch: "work/bl-f813",
        subject: "Title [bl-f813]",
        override_msg: None,
        marker: "[bl-f813]",
        target,
    }
}

/// Drive one hook against a fresh fake and return the calls it made.
fn drive(op: &str, phase: &str, rolling_back: bool) -> Vec<String> {
    drive_spec(op, phase, rolling_back, &spec())
}

/// [`drive`] with an explicit spec (the nested cases pass a target).
fn drive_spec(op: &str, phase: &str, rolling_back: bool, spec: &Spec) -> Vec<String> {
    let repo = FakeRepo::default();
    dispatch(op, phase, rolling_back, &repo, spec).unwrap();
    repo.calls()
}

#[test]
fn claim_post_materializes() {
    assert_eq!(drive("claim", "post", false), ["materialize /wt work/bl-f813"]);
}

#[test]
fn prime_post_does_not_materialize() {
    // Worktrees materialize at CLAIM only (bl-c2bf): prime is not in the
    // dispatch matrix, so it drives no `Repo` act here — the binary's prime
    // path only prunes settled branches, outside dispatch.
    assert_eq!(drive("prime", "post", false), Vec::<String>::new());
}

#[test]
fn unclaim_post_releases_and_keeps_the_branch() {
    // The counterpart to `close_post_discards_worktree_and_branch`: unclaim
    // delivered nothing, so the branch is the next claimant's starting point
    // (bl-65e0) and must survive.
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
fn close_post_discards_worktree_and_branch() {
    // bl-ce3b: the close that just delivered deletes the branch too. Deferring
    // it to prime leaked one branch forever per NESTED ball, whose delivery
    // never tags the integration branch prime scans.
    assert_eq!(drive("close", "post", false), ["discard /wt work/bl-f813"]);
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
fn a_nested_claim_mints_the_targets_ref_then_forks_its_own_off_it() {
    // bl-7b71: the target ref is minted at the integration head if the epic has
    // no branch yet (nothing to orphan — a bare ref), the child's branch is
    // minted ON it, and only then does the worktree materialize. So the child
    // starts from the work it gates instead of from clean main.
    assert_eq!(
        drive_spec("claim", "post", false, &targeted(Some("bl-epic"))),
        [
            "integration",
            "mint work/bl-epic at main",
            "mint work/bl-f813 at work/bl-epic",
            "materialize /wt work/bl-f813",
        ]
    );
}

#[test]
fn a_nested_close_delivers_into_the_targets_ref_not_the_integration_branch() {
    // "done" stops meaning "on main": it means delivered to MY target. The epic
    // accumulates its children on `work/bl-epic` and lands whole later.
    assert_eq!(
        drive_spec("close", "pre", false, &targeted(Some("bl-epic"))),
        [
            "integration",
            "mint work/bl-epic at main",
            "deliver /wt work/bl-f813 -> work/bl-epic : Title [bl-f813] : [bl-f813]",
        ]
    );
}

#[test]
fn a_flat_claim_forks_head_and_mints_nothing() {
    // The DEFAULT is untouched: no target ⇒ no integration read, no mint —
    // `worktree add -b` forks the repo's HEAD exactly as it always did.
    assert_eq!(drive("claim", "post", false), ["materialize /wt work/bl-f813"]);
}

#[test]
fn an_integration_failure_aborts_a_close() {
    let repo = FakeRepo { fail_integration: true, ..FakeRepo::default() };
    assert!(dispatch("close", "pre", false, &repo, &spec()).is_err());
}

#[test]
fn resolve_id_prefers_the_sealed_metadata_trailer() {
    let mut md = Metadata::new();
    md.insert("bl-id".into(), vec!["bl-abc1".into()]);
    assert_eq!(resolve_id(Some(&md), None).unwrap(), "bl-abc1");
}

#[test]
fn resolve_id_takes_the_ball_off_the_pre_wires_command() {
    // The pre wire has no sealed trailer, so `command.id` — the ball core named
    // at op-start — IS the identity (§0 obligation 4, bl-a5f3). Nothing is read
    // from the change worktree, so a committed-but-unintegrated failed seal
    // (worktree clean, no seal record) resolves exactly like the forward run
    // instead of erroring "found 0" and voicing a FAILED ROLLBACK.
    assert_eq!(resolve_id(None, Some("bl-9f9f")).unwrap(), "bl-9f9f");
}

#[test]
fn resolve_id_rejects_a_wire_that_names_no_ball() {
    // The plugin was wired onto an op with no ball (a §16 bulk import) — a
    // protocol error, and now the ONLY way identity can be missing.
    let err = resolve_id(None, None).unwrap_err();
    assert!(err.to_string().contains("no ball on the wire"), "{err}");
}

#[test]
fn claim_post_surfaces_the_bare_path() {
    // The verb's one product, the way `create` prints the id (§11) — printed
    // whether or not the dir pre-existed (claim.post just materialized it). It
    // is the ONLY moment a worktree surfaces: prime no longer does (bl-c2bf).
    let wt = Path::new("/wt/bl-x");
    assert_eq!(surfaced("claim", "post", false, wt, true).as_deref(), Some("/wt/bl-x"));
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
        ("prime", "post", false, true), // prime materializes nothing now (bl-c2bf)
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
