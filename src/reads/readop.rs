//! §6 read-op plugin dispatch — how `bl show` folds a plugin-computed line into
//! the HUMAN render. Dispatch is op-uniform: there is no rule that only mutating
//! ops invoke plugins. A read carries no seal and no `pre`/`post` split, so it
//! runs the `[hooks]` schedule's BARE `<op>` key as a single phase (`"read"`),
//! cwd = the store, with the §7 wire minus the task-op fields — the named ball
//! travels as `metadata.bl-id`, the same id channel a sealed post wire uses.
//! Core CAPTURES each plugin's stdout and folds it verbatim into the human
//! render; nothing is parsed back (§7 — still no return channel), and `--json`
//! (the lossless store mirror, §9) never dispatches. Any failure — a missing
//! schedule, a dangling binding, a plugin that won't spawn or exits non-zero —
//! is NON-FATAL: the read still renders, minus that plugin's line (a read
//! mutates nothing, so there is nothing to roll back).

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::checkout;
use crate::config::{self, EffectiveConfig};
use crate::edge::Edge;
use crate::hooks::Hooks;
use crate::message::{Metadata, PROTOCOL};
use crate::plugin::{retry_busy, DEPTH_CAP};
use crate::registry::Registry;
use crate::verb::Verb;
use crate::wire::OpContext;

/// The lines to fold into `verb`'s human render for ball `id`: every plugin
/// wired under the bare `<op>` hook key, run in list order, each contributing
/// its captured stdout. Empty when nothing is wired, when balls is at the §6
/// recursion cap (no further plugin may spawn), or when every contribution
/// failed — folding is best-effort by contract.
pub(crate) fn fold(edge: &Edge, store: &Path, verb: Verb, id: &str) -> String {
    if edge.depth >= DEPTH_CAP {
        return String::new();
    }
    let landing = edge.xdg.clone_dir(&edge.invocation_path).landing();
    let Ok(hooks) = Hooks::effective(&landing, &edge.xdg.user_config()) else {
        return String::new();
    };
    let refs = hooks.resolve_read(&Registry::at(&landing), verb.token());
    if refs.is_empty() {
        return String::new();
    }
    let Ok(cfg) = EffectiveConfig::resolve(&landing, &edge.xdg.user_config()) else {
        return String::new();
    };
    let remote = config::xdg_remote(&edge.xdg.user_config());
    let binding = checkout::binding(&landing, store, &edge.invocation_path, remote, cfg.tasks_branch);
    let ctx = OpContext { actor: edge.default_actor.clone(), binding, command: None, before: None };
    let metadata = Metadata::from([("bl-id".to_string(), vec![id.to_string()])]);

    let mut out = String::new();
    for plugin in refs {
        // A dangling binding (`bin/<name>` absent) skips THIS plugin's line: on a
        // mutating op it aborts (§6), but a read renders best-effort.
        let Some(bin) = plugin.bin else { continue };
        let payload = ctx.read_wire(&plugin.name, verb.token(), &metadata);
        let line = serde_json::to_string(&payload)
            .map_err(io::Error::other)
            .and_then(|json| capture(&bin, &plugin.name, verb.token(), edge.depth, store, &json));
        if let Ok(line) = line {
            out.push_str(&line);
        }
    }
    out
}

/// Spawn `<bin> <op> read` (§6 argv shape, the read's single phase) with the
/// wire on stdin and the §6 env, cwd = the store. stdout is CAPTURED — the
/// contribution to fold — unlike the mutating path's inherit; stderr is dropped
/// (a read builds no op log to envelope it into). A spawn failure or non-zero
/// exit is an error the caller treats as "no line" (§6 — non-fatal).
fn capture(bin: &Path, name: &str, op: &str, depth: u32, store: &Path, payload: &str) -> io::Result<String> {
    let mut child = retry_busy(|| {
        Command::new(bin)
            .args([op, "read"])
            .current_dir(store)
            .env("BALLS_PROTOCOL", PROTOCOL.to_string())
            .env("BALLS_PLUGIN_NAME", name)
            .env("BALLS_PLUGIN_DEPTH", (depth + 1).to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    })?;
    child.stdin.take().expect("stdin was configured as a pipe").write_all(payload.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!("plugin {name} failed the {op} read dispatch")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
#[path = "readop_tests.rs"]
mod tests;
