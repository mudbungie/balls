//! §4 TOML layer merge — read one `*.toml` layer and fold an inner layer over a
//! base, with the `<field>_prepend`/`_append`/`_ban` list-compose directives.
//! A general TOML utility, not config-specific: shared verbatim by [`super`]
//! (the `EffectiveConfig` fold) and [`crate::hooks`] (the `[hooks]` list layer —
//! one merge, not two, bl-8540). Lifted from [`super`] so the config file stays
//! the typed `EffectiveConfig` + the §12 remote ladder.

use std::fs;
use std::io;
use std::path::Path;

use toml::value::{Table, Value};

/// Read one `config/balls.toml` layer. Absent ⇒ `None` (the layer is empty, the
/// common un-configured case); malformed ⇒ an error naming the file; any other
/// read error propagates. Shared with [`crate::conf`], whose provenance read
/// inspects each layer table individually (§4).
pub(crate) fn read_layer(path: &Path) -> io::Result<Option<Table>> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str::<Table>(&text)
            .map(Some)
            .map_err(|e| io::Error::other(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Apply §4 merge of `inner` over `base`, `inner` winning. A `<field>_prepend`/
/// `_append`/`_ban` key composes the list at `<field>`; every other key (scalar
/// or object) fully replaces its `base` entry. Shared with [`crate::hooks`], which
/// layers the `[hooks]` lists the SAME way (§4/§6, bl-8540) — one merge, not two.
pub(crate) fn layer_over(base: &mut Table, inner: Table) {
    for (key, value) in inner {
        match list_directive(&key) {
            Some((field, op)) => compose_list(base, field, op, value),
            None => {
                base.insert(key, value);
            }
        }
    }
}

/// The list-compose directives (§4). A bare `<field>` is plain replacement and
/// is NOT a directive — only these three suffixes compose.
#[derive(Clone, Copy)]
pub(super) enum ListOp {
    Prepend,
    Append,
    Ban,
}

/// Split a `<field><suffix>` key into its target field and compose op, or `None`
/// for a plain key. A bare suffix with no field (`_append`) is not a directive.
pub(super) fn list_directive(key: &str) -> Option<(&str, ListOp)> {
    const SUFFIXES: [(&str, ListOp); 3] = [
        ("_prepend", ListOp::Prepend),
        ("_append", ListOp::Append),
        ("_ban", ListOp::Ban),
    ];
    SUFFIXES.into_iter().find_map(|(suffix, op)| {
        key.strip_suffix(suffix)
            .filter(|field| !field.is_empty())
            .map(|field| (field, op))
    })
}

/// Compose `value` (treated as a list) into the list at `base[field]` per `op`:
/// prepend/append concatenate; ban removes every element `value` contains. A
/// non-list current value (or incoming) is treated as empty — the directive
/// then seeds the field, and a type clash surfaces at projection (§4).
fn compose_list(base: &mut Table, field: &str, op: ListOp, value: Value) {
    let incoming = as_array(&value);
    let current = base.get(field).map(as_array).unwrap_or_default();
    let merged: Vec<Value> = match op {
        ListOp::Prepend => incoming.into_iter().chain(current).collect(),
        ListOp::Append => current.into_iter().chain(incoming).collect(),
        ListOp::Ban => current.into_iter().filter(|v| !incoming.contains(v)).collect(),
    };
    base.insert(field.to_string(), Value::Array(merged));
}

/// A TOML value's elements if it is an array, else empty — the lenient read the
/// list directives share (a clash becomes a projection error, not a panic).
fn as_array(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
