//! Input-side §7 payload — the slice of the wire the tracker reads back.
//!
//! [`crate::wire`] is output-only: balls serializes a payload to a plugin's
//! stdin and never deserializes one (the §7 no-return-channel rule). The
//! tracker is the SEPARATE binary on the receiving end, so it owns its own
//! input type — and it needs exactly the `binding`, "exactly what a fetcher
//! needs" (§7). The op and phase arrive on argv (§6 `<bin> <op> <phase>`), so
//! the payload contributes only the binding; every other wire field is ignored
//! by serde, which keeps this type stable as the wire grows.

use serde::Deserialize;
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

/// Just enough of the §7 envelope to reach the `binding`; serde drops the rest.
#[derive(Deserialize)]
struct Envelope {
    binding: Binding,
}

/// Read the §7 payload JSON from `input` and return its [`Binding`]. Unparseable
/// JSON (or a payload with no `binding`) is an [`io::Error`] — the plugin aborts
/// the op, exactly as a non-zero exit does for any other failure (§6).
pub fn read_binding(input: &mut impl Read) -> io::Result<Binding> {
    let mut buf = String::new();
    input.read_to_string(&mut buf)?;
    let envelope: Envelope = serde_json::from_str(&buf).map_err(io::Error::other)?;
    Ok(envelope.binding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: &str) -> io::Result<Binding> {
        read_binding(&mut json.as_bytes())
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
