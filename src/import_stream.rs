//! `bl import`'s stdin grammar — bedrock JSON records (§9) back into
//! `(id, Task)` pairs, the exact inverse of `task_json`. STRICT: a record is a
//! JSON object carrying a valid string `id`, the §3 canonical fields typed as
//! the bedrock emits them (`created`/`updated` required — the record is
//! fully-identified or refused), and a string `body`; unknown keys land in the
//! preserved `extra` table like any stored frontmatter (§3). Anything else is
//! an error naming the record — refuse, don't guess.

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
    task.body = body;
    Ok((id, task))
}

#[cfg(test)]
#[path = "import_stream_tests.rs"]
mod tests;
