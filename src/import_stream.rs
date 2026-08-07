//! `bl import`'s stdin grammar — bedrock JSON records (§9) back into
//! `(id, Task)` pairs, the exact inverse of `task_json`. STRICT: a record is a
//! JSON object carrying a valid string `id`, the §3 canonical fields typed as
//! the bedrock emits them (`created`/`updated` required — the record is
//! fully-identified or refused), and a string `body`; unknown keys land in the
//! preserved `extra` table like any stored frontmatter (§3). Every id the
//! record carries — its own AND its `parent`/`blockers[].id` edges — must be a
//! safe path token ([`require_edge_shapes`]). Anything else is an error naming
//! the record — refuse, don't guess.

use std::io;

use serde_json::Value;

use crate::id;
use crate::task::Task;
use crate::taskfile::invalid;

/// Parse the whole stdin text into records. The grammar is whatever the
/// bedrock readers emit: one object (`show --json`), an array (`list --json`),
/// or any concatenation of those (a stream of `show` outputs) — arrays are
/// flattened one level, so a pipe needs no joining filter.
pub(super) fn records(text: &str) -> io::Result<Vec<(String, Task)>> {
    let mut out = Vec::new();
    for value in serde_json::Deserializer::from_str(text).into_iter::<Value>() {
        let value = value.map_err(|e| invalid(format!("import: bad JSON on stdin: {e}")))?;
        match value {
            Value::Array(items) => {
                for item in items {
                    out.push(from_record(item)?);
                }
            }
            item => out.push(from_record(item)?),
        }
    }
    Ok(out)
}

/// One bedrock record → `(id, Task)`. `id` (the filename identity) and `body`
/// (not frontmatter) are peeled off the object; the §3 frontmatter fields —
/// extras included — deserialize through the same serde shape `task.rs`
/// stores, so the file `import` writes is byte-what-`show --json` mirrored.
fn from_record(mut value: Value) -> io::Result<(String, Task)> {
    let Some(record) = value.as_object_mut() else {
        return Err(invalid(format!("import: a record must be a JSON object, got: {value}")));
    };
    let id = match record.remove("id") {
        Some(Value::String(id)) if id::is_valid(&id) => id,
        Some(other) => return Err(invalid(format!("import: invalid id {other}"))),
        None => return Err(invalid("import: a record needs an \"id\"".to_string())),
    };
    let body = match record.remove("body") {
        Some(Value::String(body)) => body,
        None => String::new(),
        Some(other) => return Err(invalid(format!("import: {id}: \"body\" must be a string, got: {other}"))),
    };
    let mut task: Task =
        serde_json::from_value(value).map_err(|e| invalid(format!("import: {id}: {e}")))?;
    require_edge_shapes(&id, &task)?;
    task.body = body;
    Ok((id, task))
}

/// EVERY id in a record must be a safe path token (§9, bl-6c19) — the record's
/// own (checked above, it names the file written) and each edge id it carries.
/// `parent` is read back through `tasks/<parent>.md` ([`crate::target::derive`])
/// and a blocker id through that file's existence ([`crate::enforce`]), so a
/// `/` or `..` in either escapes the store on READ, one hop after the import
/// that took it verbatim. SHAPE only, never liveness (bl-1fc4's line): a
/// DANGLING edge imports fine — that is the record a peer, a backup or a
/// partial stream legitimately carries, and refusing it would be the enforce
/// gate §9 says `import` does not have.
fn require_edge_shapes(id: &str, task: &Task) -> io::Result<()> {
    let parent = task.parent.iter().map(|p| ("parent", p));
    for (what, edge) in parent.chain(task.blockers.iter().map(|b| ("blocker", &b.id))) {
        if !id::is_valid(edge) {
            return Err(invalid(format!(
                "import: {id}: {what} '{edge}' is not a task id — an id is a filename (§3), so every id in a record must be a safe path token"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "import_stream_tests.rs"]
mod tests;
