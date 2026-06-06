//! Tests for the pure §12 trail walk — driven by in-memory `next` resolvers, no
//! filesystem, so every branch (trail-end, multi-hop, revisit) is exercised on
//! the one compiled `walk`.

use super::walk;
use std::collections::HashMap;
use std::path::PathBuf;

/// Build a `next` resolver from explicit `from → to` edges.
fn edges(pairs: &[(&str, &str)]) -> HashMap<PathBuf, PathBuf> {
    pairs.iter().map(|(a, b)| (PathBuf::from(a), PathBuf::from(b))).collect()
}

#[test]
fn a_pointerless_landing_is_a_one_node_trail() {
    let trail = walk(PathBuf::from("/landing"), &mut |_| None);
    assert_eq!(trail, vec![PathBuf::from("/landing")]);
}

#[test]
fn it_follows_pointers_transitively_to_the_terminus() {
    let map = edges(&[("/landing", "/step"), ("/step", "/terminus")]);
    let trail = walk(PathBuf::from("/landing"), &mut |c| map.get(c).cloned());
    assert_eq!(
        trail,
        vec![
            PathBuf::from("/landing"),
            PathBuf::from("/step"),
            PathBuf::from("/terminus"),
        ]
    );
    // The terminus (task store) is the last node — what `bl sync` syncs.
    assert_eq!(trail.last().unwrap(), &PathBuf::from("/terminus"));
}

#[test]
fn it_stops_on_a_revisit_rather_than_looping() {
    // /a → /b → /a … a 2-cycle that would spin forever without the guard.
    let map = edges(&[("/a", "/b"), ("/b", "/a")]);
    let trail = walk(PathBuf::from("/a"), &mut |c| map.get(c).cloned());
    assert_eq!(trail, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
}

#[test]
fn it_stops_when_a_trail_converges_back_on_the_start() {
    // A center two hops out points back at the landing (in-degree convergence).
    let map = edges(&[("/l", "/c"), ("/c", "/l")]);
    let trail = walk(PathBuf::from("/l"), &mut |c| map.get(c).cloned());
    assert_eq!(trail, vec![PathBuf::from("/l"), PathBuf::from("/c")]);
}
