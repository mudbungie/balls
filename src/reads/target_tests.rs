//! Tests for the rendered delivery-target column (bl-6915): the catalog-derived
//! target of a row, and the `  ->bl-xxxx` marker it renders as.

use super::*;
use crate::reads::test_support::{blocker, catalog, task};
use crate::task::On;

/// A ball with `parent`, and a parent carrying `blockers`.
fn nested(parent_blockers: Vec<crate::task::Blocker>) -> Catalog {
    let mut kid = task("kid", 1);
    kid.parent = Some("bl-epic".into());
    let mut epic = task("epic", 0);
    epic.blockers = parent_blockers;
    catalog(&[("bl-kid", kid), ("bl-epic", epic)])
}

/// A parentless ball delivers flat — no target, no marker. The overwhelming
/// case, and the one that must render byte-identically to pre-nesting `bl list`.
#[test]
fn parentless_has_no_target() {
    let cat = catalog(&[("bl-kid", task("kid", 1))]);
    let t = &cat.get("bl-kid").unwrap().task;
    assert_eq!(of(&cat, "bl-kid", t), None);
    assert_eq!(row_marker(&cat, "bl-kid", t), "");
}

/// A `parent` pointing at a ball that is not live (closed, or never was) is a
/// dangling display-only pointer — it gates nothing, so delivery is flat. This
/// is also the LANDED reading of a closed ball: parent closed ⇒ no marker.
#[test]
fn dead_parent_gates_nothing() {
    let mut kid = task("kid", 1);
    kid.parent = Some("bl-gone".into());
    let cat = catalog(&[("bl-kid", kid)]);
    let t = &cat.get("bl-kid").unwrap().task;
    assert_eq!(of(&cat, "bl-kid", t), None);
}

/// Bare containment is NOT nesting: a live parent that does not close-gate this
/// ball leaves it delivering to the integration branch (bl-7b71 — nesting needs
/// both coordinates). A close-gate on some OTHER ball does not count either.
#[test]
fn containment_alone_is_flat() {
    let cat = nested(vec![blocker("bl-other", On::Close)]);
    let t = &cat.get("bl-kid").unwrap().task;
    assert_eq!(of(&cat, "bl-kid", t), None);
    assert_eq!(row_marker(&cat, "bl-kid", t), "");
}

/// A claim-gate on the parent is the OLD `--subtask-of` shape — enforcement, not
/// nesting; it must not redirect delivery.
#[test]
fn claim_gate_is_not_nesting() {
    let cat = nested(vec![blocker("bl-kid", On::Claim)]);
    let t = &cat.get("bl-kid").unwrap().task;
    assert_eq!(of(&cat, "bl-kid", t), None);
}

/// Both coordinates — `parent = X` and `{this, on: close}` on X — is the nesting
/// declaration, and the marker is the rendered column.
#[test]
fn close_gated_child_renders_its_target() {
    let cat = nested(vec![blocker("bl-kid", On::Close)]);
    let t = &cat.get("bl-kid").unwrap().task;
    assert_eq!(of(&cat, "bl-kid", t), Some("bl-epic"));
    assert_eq!(row_marker(&cat, "bl-kid", t), "  ->bl-epic");
}
