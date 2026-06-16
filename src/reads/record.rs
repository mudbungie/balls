//! The **bedrock** JSON record (§9) — the single machine shape every read
//! verb's `--json` emits, and (since bl-e614) the shape `bl import` ingests
//! back. Lifted to a sibling of [`super`] so the machine contract has one
//! file-sized home.

use serde_json::{json, Value};

use crate::task::{Status, Task};

/// The §3 status word — the stable token shared by plain output and `--json`.
pub(crate) fn status_word(s: Status) -> &'static str {
    match s {
        Status::Ready => "ready",
        Status::Claimed => "claimed",
        Status::Blocked => "blocked",
    }
}

/// One ball as the **bedrock** JSON record — the single shape every read verb's
/// `--json` emits (§9). It is the lossless mirror of the stored `tasks/<id>.md`
/// FILE ONLY: every stored field — the frontmatter AND the markdown `body` —
/// round-trips back to the file, and NOTHING derived appears — no `status`
/// ladder, no ISO dates (timestamps stay the literal stored i64), no
/// inverse-derived `children`, no tree nesting. The derived columns live on the
/// orthogonal HUMAN render alone (§3, bl-d074). `id` is the filename identity
/// (the round-trip key), not a frontmatter field. The record is total — `bl
/// import` writes it back verbatim (bl-e614), so what `show --json` reads out
/// must be the whole ball.
///
/// Preserved `extra` keys (§3 seam — a team's `state:` field) ride through too:
/// lossless means EVERY stored key. And ONLY stored keys: a plugin-computed
/// value (the delivery worktree path, §11) is never here — `--json` never
/// dispatches a read-op plugin (§6), it mirrors the file alone. Extras are
/// UNKNOWN keys, so none can collide with the canonical fields layered over
/// them; the canonical set is always present (a cleared scalar emits `null`,
/// an empty body `""`), unlike the file's skip-if-absent frontmatter.
pub(crate) fn task_json(id: &str, task: &Task) -> Value {
    let blockers: Vec<Value> = task
        .blockers
        .iter()
        .map(|b| json!({ "id": b.id, "on": b.on.token() }))
        .collect();
    let mut record = serde_json::to_value(&task.extra).expect("a toml table serializes to a json object");
    let map = record.as_object_mut().expect("a toml table is a json object");
    for (key, value) in [
        ("id", json!(id)),
        ("title", json!(task.title)),
        ("claimant", json!(task.claimant)),
        ("priority", json!(task.priority)),
        ("parent", json!(task.parent)),
        ("tags", json!(task.tags)),
        ("blockers", json!(blockers)),
        ("created", json!(task.created)),
        ("updated", json!(task.updated)),
        ("body", json!(task.body)),
    ] {
        map.insert(key.to_string(), value);
    }
    record
}

/// Serialise a JSON value to a trailing-newline string — the one place every
/// `--json` branch renders, so the machine contract is byte-identical.
pub(crate) fn json_line(v: &Value) -> String {
    format!("{}\n", serde_json::to_string_pretty(v).expect("serde_json::Value serializes"))
}
