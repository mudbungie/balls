//! §4 `bl conf` writes — scope-keyed CRUD on each key's canonical home.
//!
//! `set` replaces (a scalar, or a hooks key's whole list); `append`/`prepend`/
//! `remove` compose a list — the §4 directive vocabulary APPLIED AT WRITE TIME
//! to the canonical bare list, never stored as `_append`/`_prepend`/`_ban` keys
//! beside it (one fact, one home; the directive keys remain the cross-LAYER
//! compose for a hand-written XDG overlay). Compose converges (§13 idempotence):
//! appending a present name or removing an absent one is a no-op, and a list
//! emptied by `remove` drops its key (absent/empty = run nothing, §4/§6).
//!
//! Homes: `task-remote` ⇒ the XDG `config.toml` (a plain per-machine file
//! edit); `task-branch`/`log-level` ⇒ the landing `balls.toml` and the hooks
//! keys ⇒ the landing `plugins.toml`, each an ordinary commit on `balls/config`
//! (`balls: conf <op> <key> …`, checkout-scoped — §5). A write that changes
//! nothing seals nothing: git's own empty-diff check is the change detector,
//! the same trick as the §8 no-op seal. Foreign tables in either TOML file
//! round-trip untouched — this edits ONE key, never re-shapes the document.

use super::Key;
use crate::edge::Edge;
use crate::git;
use crate::layout::CloneDir;
use crate::log::Level;
use crate::message::Message;
use crate::verb::Verb;
use std::fs;
use std::io;
use std::path::Path;
use toml::value::{Table, Value};

/// Dispatch one write: `bl conf <op> <key> <value...>`. The key implies its
/// home (§4); a list op on a scalar key is refused, naming the split.
pub(super) fn run(edge: &Edge, clone: &CloneDir, op: &str, rest: &[String]) -> io::Result<()> {
    let Some((token, values)) = rest.split_first() else {
        return Err(io::Error::other(format!("conf {op}: needs <key> <value...>")));
    };
    let landing = clone.landing();
    let actor = &edge.default_actor;
    match (Key::parse(token)?, op) {
        (Key::Hook(k), _) => hooks_edit(&landing, actor, op, &k, values),
        (Key::TaskRemote, "set") => xdg_set(edge, one(op, token, values)?),
        (Key::LogLevel, "set") => {
            let value = one(op, token, values)?;
            Level::parse(value)?; // refuse a level the ladder won't speak
            landing_set(&landing, actor, token, "log_level", value)
        }
        (Key::TaskBranch, "set") => landing_set(&landing, actor, token, "tasks_branch", one(op, token, values)?),
        _ => Err(io::Error::other(format!(
            "conf {op}: '{token}' is a scalar — append/prepend/remove compose the [hooks] list keys"
        ))),
    }?;
    eprintln!("conf {op} {token}");
    Ok(())
}

/// The single value a scalar `set` takes.
fn one<'a>(op: &str, key: &str, values: &'a [String]) -> io::Result<&'a str> {
    match values {
        [only] => Ok(only),
        _ => Err(io::Error::other(format!("conf {op}: '{key}' takes exactly one value"))),
    }
}

/// Set the per-machine XDG `remote` (§12) — a plain file edit on the user
/// config, every other key in it untouched. The one home that is not the
/// landing: a remote URL must not travel on `install` (§4).
fn xdg_set(edge: &Edge, url: &str) -> io::Result<()> {
    let path = edge.xdg.user_config();
    let mut table = read_table(&path)?;
    table.insert("remote".into(), Value::String(url.to_string()));
    fs::create_dir_all(path.parent().expect("the XDG user config always has a parent"))?;
    fs::write(&path, toml::to_string(&Value::Table(table)).expect("a string field always serializes"))
}

/// Set a landing `balls.toml` scalar and seal it on `balls/config` (§4).
fn landing_set(landing: &Path, actor: &str, token: &str, field: &str, value: &str) -> io::Result<()> {
    edit_landing_toml(landing, actor, "balls.toml", &format!("balls: conf set {token} {value}"), |table| {
        table.insert(field.into(), Value::String(value.to_string()));
        Ok(())
    })
}

/// Apply one §4 list op to a `[hooks]` key on the landing `plugins.toml`:
/// `set` bare-replaces with `values`; `append`/`prepend` insert ONE name iff
/// absent (convergent); `remove` prunes it, dropping the key when emptied.
fn hooks_edit(landing: &Path, actor: &str, op: &str, key: &str, values: &[String]) -> io::Result<()> {
    if op != "set" {
        one(op, key, values)?; // compose moves exactly one name
    }
    let subject = format!("balls: conf {op} {key} {}", values.join(" "));
    edit_landing_toml(landing, actor, "plugins.toml", &subject, |root| {
        let Value::Table(hooks) = root.entry("hooks").or_insert_with(|| Value::Table(Table::new())) else {
            return Err(io::Error::other("plugins.toml: [hooks] is not a table"));
        };
        let mut names: Vec<String> = hooks
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        match op {
            "set" => names = values.to_vec(),
            "append" if !names.contains(&values[0]) => names.push(values[0].clone()),
            "prepend" if !names.contains(&values[0]) => names.insert(0, values[0].clone()),
            "remove" => names.retain(|n| n != &values[0]),
            _ => {} // append/prepend of a present name — the convergent no-op
        }
        if names.is_empty() {
            hooks.remove(key); // absent/empty = run nothing (§4) — drop, don't store []
        } else {
            let list = names.into_iter().map(Value::String).collect();
            hooks.insert(key.to_string(), Value::Array(list));
        }
        Ok(())
    })
}

/// Read-edit-write one landing `config/<file>` TOML document, then seal it as
/// an ordinary commit on `balls/config` carrying the §5 checkout-scoped
/// trailer block (bl-1d9b). The edit touches one key; everything else in the
/// document round-trips. A no-change edit commits nothing — git's empty-diff
/// check is the §13 convergence test.
fn edit_landing_toml(
    landing: &Path,
    actor: &str,
    file: &str,
    subject: &str,
    edit: impl FnOnce(&mut Table) -> io::Result<()>,
) -> io::Result<()> {
    let path = landing.join("config").join(file);
    let mut table = read_table(&path)?;
    edit(&mut table)?;
    fs::write(&path, toml::to_string(&Value::Table(table)).expect("a hooks/scalar table always serializes"))?;
    git::run(landing, &["add", "-A", "config"], None)?;
    if git::run(landing, &["diff", "--cached", "--quiet"], None).is_ok() {
        return Ok(()); // the value already held — converge, no empty commit
    }
    let message = Message::checkout(Verb::Conf, actor, subject.to_string()).render()?;
    git::run(landing, &["commit", "-q", "-F", "-"], Some(&message))?;
    Ok(())
}

/// One TOML document as a table: absent ⇒ empty (the un-configured case),
/// malformed ⇒ an error naming the file ([`crate::config::read_layer`]).
fn read_table(path: &Path) -> io::Result<Table> {
    Ok(crate::config::read_layer(path)?.unwrap_or_default())
}

#[cfg(test)]
#[path = "conf_write_tests.rs"]
mod tests;
