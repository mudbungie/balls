//! Input-side §7 payload — the slice of the wire the tracker reads back.
//!
//! [`crate::wire`] is output-only: balls serializes a payload to a plugin's
//! stdin and never deserializes one (the §7 no-return-channel rule). The
//! tracker is the SEPARATE binary on the receiving end, so it owns its own
//! input type — the `binding`, "exactly what a fetcher needs" (§7), plus the
//! op's BALL for the drift render (bl-439d): `command.id` on a mutating wire,
//! `metadata.bl-id` on a read wire (§6 — a read carries no `command`). The op
//! and phase arrive on argv (§6 `<bin> <op> <phase>`); every other wire field
//! is ignored by serde, which keeps this type stable as the wire grows.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{self, Read};

/// §7 binding — where the op is happening, from the tracker's seat. `remote` is
/// absent in a stealth (no-remote) repo, which is the tracker's whole branch
/// point: no remote ⇒ nothing to talk to. `stealth` is the §12 declared opt-out
/// (the landing `task_remote` sentinel, derived by core per op — bl-9df0): it
/// makes the no-remote read DECLARED rather than
/// inferred, suppressing even `origin` discovery (absent on every ordinary
/// payload, so it defaults `false`). `store` is the STORE checkout it
/// fetches/pushes `tasks_branch` against (§2); `landing` is the `balls/config`
/// checkout the `install/pre` config fetch targets (§6/§13) and the W2 gap's
/// durable-ladder read (bl-9df0 — every other handler
/// ignores it, so it defaults empty); `invocation_path` locates the project
/// repo whose `origin` is the implicit bottom tier (§12).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Binding {
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub stealth: bool,
    pub tasks_branch: String,
    pub store: String,
    #[serde(default)]
    pub landing: String,
    pub invocation_path: String,
}

/// What the tracker reads off one payload: the binding, and the ball the op is
/// about when it is about one (`None` on `sync`/`prime`/`install`, the bulk
/// `import`, and `list`).
pub struct Input {
    pub binding: Binding,
    pub id: Option<String>,
}

/// Just enough of the §7 envelope to reach the `binding` and the ball id;
/// serde drops the rest.
#[derive(Deserialize)]
struct Envelope {
    binding: Binding,
    #[serde(default)]
    command: Option<Command>,
    #[serde(default)]
    metadata: Option<BTreeMap<String, Vec<String>>>,
}

/// The one `command` field the tracker reads: the ball (§7, carried not derived).
#[derive(Deserialize)]
struct Command {
    #[serde(default)]
    id: Option<String>,
}

/// Read the §7 payload JSON from `input`. Unparseable JSON (or a payload with
/// no `binding`) is an [`io::Error`] — the plugin aborts the op, exactly as a
/// non-zero exit does for any other failure (§6).
pub fn read_input(input: &mut impl Read) -> io::Result<Input> {
    let mut buf = String::new();
    input.read_to_string(&mut buf)?;
    let e: Envelope = serde_json::from_str(&buf).map_err(io::Error::other)?;
    let from_trailer = e.metadata.and_then(|m| m.get("bl-id").and_then(|v| v.first().cloned()));
    Ok(Input { binding: e.binding, id: e.command.and_then(|c| c.id).or(from_trailer) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: &str) -> io::Result<Binding> {
        read_input(&mut json.as_bytes()).map(|i| i.binding)
    }

    #[test]
    fn the_op_ball_comes_from_command_id_or_the_read_wires_trailer() {
        // bl-439d: a mutating wire names the ball in `command.id`; a §6 read
        // wire has no command and names it in `metadata.bl-id`; a diffless op
        // names none.
        let b = r#""binding":{"tasks_branch":"b","store":"/s","invocation_path":"/p"}"#;
        let with_cmd = format!(r#"{{{b},"command":{{"op":"claim","id":"bl-0001"}}}}"#);
        assert_eq!(read_input(&mut with_cmd.as_bytes()).unwrap().id.as_deref(), Some("bl-0001"));
        let read_wire = format!(r#"{{{b},"metadata":{{"bl-id":["bl-0002"]}}}}"#);
        assert_eq!(read_input(&mut read_wire.as_bytes()).unwrap().id.as_deref(), Some("bl-0002"));
        let diffless = format!("{{{b}}}");
        assert!(read_input(&mut diffless.as_bytes()).unwrap().id.is_none());
    }

    #[test]
    fn reads_a_tracked_binding_and_ignores_extra_wire_fields() {
        let b = read(
            r#"{"op":"sync","phase":"pre","actor":"x","command":{"op":"sync"},
                "binding":{"remote":"git@h:r","tasks_branch":"balls/tasks",
                           "store":"/store","landing":"/landing","invocation_path":"/proj"}}"#,
        )
        .unwrap();
        assert_eq!(b.remote.as_deref(), Some("git@h:r"));
        assert_eq!(b.tasks_branch, "balls/tasks");
        assert_eq!(b.store, "/store");
        assert_eq!(b.landing, "/landing");
        assert_eq!(b.invocation_path, "/proj");
        assert!(!b.stealth); // absent on an ordinary payload — defaults false
    }

    #[test]
    fn an_absent_remote_or_landing_is_the_stealth_binding() {
        // A stealth payload omits both remote and landing — each defaults.
        let b = read(
            r#"{"binding":{"tasks_branch":"balls/tasks","store":"/store","invocation_path":"/p"}}"#,
        )
        .unwrap();
        assert_eq!(b.remote, None);
        assert_eq!(b.landing, "");
    }

    #[test]
    fn an_explicit_stealth_flag_rides_the_binding() {
        // `bl prime --stealth` (§12): the declared opt-out arrives as a field.
        let b = read(
            r#"{"binding":{"stealth":true,"tasks_branch":"balls/tasks","store":"/store","invocation_path":"/p"}}"#,
        )
        .unwrap();
        assert!(b.stealth);
        assert_eq!(b.remote, None);
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(read("not json").is_err());
    }
}
