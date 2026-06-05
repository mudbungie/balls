//! The default §10 gating plugin — the SOLE enforcer of the blocker model.
//!
//! Core stores `blockers` and enforces NOTHING (§10): a blocked ball is still
//! claimable/closeable in naked core. This tiny plugin is what actually rejects.
//! Wired at `claim.pre` and `close.pre`, it reads the §7 payload on stdin and,
//! for the blocker `on` matching the op (claim→claim-blockers, close→close-
//! blockers), rejects the op (exit 1) if any named task is still UNRESOLVED —
//! i.e. its `tasks/<id>.md` still exists in the change worktree ("resolved" =
//! closed or dropped = file gone, §10). It contributes no CLI and reads nothing
//! but its own blockers; the resolution mechanism (build/forge/human) lives in
//! whatever closes the gate child, never here.

use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::message::PROTOCOL;
use crate::task::{Blocker, On};
use crate::taskfile;

/// The slice of the §7 payload the gate reads: the blocked task's blockers.
/// Every other wire field is ignored (serde drops unknown keys).
#[derive(Deserialize)]
struct Payload {
    current_state: Option<State>,
}

#[derive(Deserialize)]
struct State {
    #[serde(default)]
    blockers: Vec<Blocker>,
}

/// The blocker `on` an op gates. Only `claim`/`close` gate; the plugin is wired
/// only for those, so any other op is a no-op (nothing to enforce).
fn gated(op: &str) -> Option<On> {
    match op {
        "claim" => Some(On::Claim),
        "close" => Some(On::Close),
        _ => None,
    }
}

/// Run the gate. `protocol` prints the §6 self-description; `<op> <phase>` reads
/// the §7 payload from `input` and enforces against `dir` (the change worktree).
/// Exit code: 0 = allow, 1 = blocked, 2 = bad invocation/payload.
pub fn run(args: &[String], input: &mut dyn Read, dir: &Path) -> i32 {
    match args.first().map(String::as_str) {
        Some("protocol") => {
            println!("{}", serde_json::json!({ "protocol": [PROTOCOL], "ops": ["claim", "close"] }));
            0
        }
        Some(op) => enforce(op, input, dir),
        None => {
            eprintln!("usage: gate <op> <phase>");
            2
        }
    }
}

/// Reject (1) if any blocker gating `op` is unresolved; allow (0) otherwise.
fn enforce(op: &str, input: &mut dyn Read, dir: &Path) -> i32 {
    let Some(on) = gated(op) else { return 0 };
    let mut raw = String::new();
    if let Err(e) = input.read_to_string(&mut raw) {
        eprintln!("gate: cannot read payload: {e}");
        return 2;
    }
    let payload: Payload = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("gate: malformed payload: {e}");
            return 2;
        }
    };
    let blockers = payload.current_state.map(|s| s.blockers).unwrap_or_default();
    let open: Vec<&str> = blockers
        .iter()
        .filter(|b| b.on == on && taskfile::exists(dir, &b.id))
        .map(|b| b.id.as_str())
        .collect();
    if open.is_empty() {
        0
    } else {
        eprintln!("gate: {op} blocked by unresolved {}", open.join(", "));
        1
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
