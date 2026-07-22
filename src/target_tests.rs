//! The §11 delivery-target derivation (bl-7b71): the pure close-gate predicate,
//! and [`derive`] over a throwaway store — both coordinates required, a dead
//! parent gating nothing.

use super::*;

use crate::task::Blocker;
use crate::taskfile::write_task;
use tempfile::TempDir;

/// A ball with `parent` and the given close-gating children.
fn ball(parent: Option<&str>, gates: &[&str]) -> Task {
    Task {
        title: "T".into(),
        parent: parent.map(str::to_string),
        blockers: gates.iter().map(|id| Blocker { id: (*id).to_string(), on: Verb::Close }).collect(),
        ..Task::default()
    }
}

/// A store dir holding `tasks/<id>.md` for each named ball.
fn store(balls: &[(&str, Task)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    for (id, task) in balls {
        write_task(tmp.path(), id, task).unwrap();
    }
    tmp
}

#[test]
fn close_gating_the_parent_is_what_declares_nesting() {
    // The edge lives on the PARENT and must name THIS ball, on `close`.
    assert!(close_gated("bl-kid", &ball(None, &["bl-kid"])));
    assert!(!close_gated("bl-kid", &ball(None, &["bl-other"]))); // a sibling's gate
    assert!(!close_gated("bl-kid", &ball(None, &[]))); // containment only
    // An edge on another OP is an ordinary dependency, not a nesting declaration.
    let claim_gated = Task { blockers: vec![Blocker { id: "bl-kid".into(), on: Verb::Claim }], ..Task::default() };
    assert!(!close_gated("bl-kid", &claim_gated));
}

#[test]
fn a_ball_close_gating_its_live_parent_targets_that_parent() {
    let tmp = store(&[("bl-epic", ball(None, &["bl-kid"]))]);
    let kid = ball(Some("bl-epic"), &[]);
    assert_eq!(derive(tmp.path(), Some(&"bl-kid".to_string()), Some(&kid)), Some("bl-epic".into()));
}

#[test]
fn nesting_needs_both_coordinates_so_either_alone_delivers_flat() {
    // Containment without a gate: the parent pointer alone is display-only.
    let ungated = store(&[("bl-epic", ball(None, &[]))]);
    let kid = ball(Some("bl-epic"), &[]);
    assert_eq!(derive(ungated.path(), Some(&"bl-kid".to_string()), Some(&kid)), None);
    // A close-gate on a NON-parent is pure enforcement — two sibling features
    // ordered by a gate keep delivering independently to the integration branch.
    let gated = store(&[("bl-other", ball(None, &["bl-kid"]))]);
    let orphan = ball(None, &[]);
    assert_eq!(derive(gated.path(), Some(&"bl-kid".to_string()), Some(&orphan)), None);
}

#[test]
fn a_closed_parent_gates_nothing_and_create_has_no_ball_at_all() {
    // The parent's file is gone (closed) — a dangling, display-only pointer, so
    // the ball delivers flat rather than at a ref nobody is accumulating onto.
    let empty = store(&[]);
    let kid = ball(Some("bl-gone"), &[]);
    assert_eq!(derive(empty.path(), Some(&"bl-kid".to_string()), Some(&kid)), None);
    // `create` carries no op-start ball: nothing to derive from.
    assert_eq!(derive(empty.path(), Some(&"bl-kid".to_string()), None), None);
    // No positional id (defensive): nothing can be gated on.
    let live = store(&[("bl-epic", ball(None, &["bl-kid"]))]);
    assert_eq!(derive(live.path(), None, Some(&ball(Some("bl-epic"), &[]))), None);
}
