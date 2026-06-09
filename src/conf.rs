//! §4/§9 `bl conf` — local config CRUD with provenance (bl-c2de).
//!
//! The §4 "by you" path made a surface: READ resolves every config value across
//! the §4 layers and reports WHICH layer answered plus the file paths behind
//! them (the "where are my files / what remote am I actually using" answer —
//! stealth shows as `task-remote (none)`, closing the bl-d234 invisible-stealth
//! gap); WRITE is scope-keyed CRUD where the KEY implies its canonical home
//! (no `--scope` flag): `task-remote` is per-machine (XDG `config.toml`, its
//! only legal home — a URL must not travel on `install`), `task-branch`/
//! `log-level` live on the landing `balls.toml`, and the `[hooks]` schedule
//! keys on the landing `plugins.toml` ([`write`], the sibling module).
//!
//! `conf` is Diffless (§8) and CHAINLESS: it authors no ball diff, seals
//! nothing to the store, and dispatches NO plugin — config never syncs (§12),
//! so a conf edit is purely local and there is nothing to react to. It cannot
//! cross a checkout boundary (that is `install`'s consent-gated job, §6) and
//! never touches a binary (the `bin/<name>` RCE gate is unchanged, §4).
//!
//! The provenance read names the `origin` tier via a LOCAL `git remote
//! get-url` (§12): naming the remote is a local config fact; *contacting* one
//! stays the tracker's alone (§0).

use crate::config;
use crate::edge::Edge;
use crate::hooks::Hooks;
use crate::layout::CloneDir;
use crate::verb::Verb;
use std::io;
use std::path::Path;

#[path = "conf_write.rs"]
mod write;

/// The §4 key namespace `conf` reads and writes. A key carries its canonical
/// home (the table in §4): the three scalars name built-in fields, [`Key::Hook`]
/// is any `[hooks]` schedule key — `<op>.<pre|post>` for the phased ops, or the
/// bare `show`/`list` a read op dispatches (§6).
#[derive(Debug)]
pub(crate) enum Key {
    /// The per-machine store remote — the XDG `remote` field (§12), the one key
    /// whose home is NOT the landing (a URL must not travel on `install`, §4).
    TaskRemote,
    /// `tasks_branch` on the landing `balls.toml` (§4).
    TaskBranch,
    /// `log_level` on the landing `balls.toml` (§4).
    LogLevel,
    /// A `[hooks]` schedule key on the landing `plugins.toml` (§6) — the only
    /// list config (§4).
    Hook(String),
}

impl Key {
    /// Resolve a key token, or the §4-namespace error. A hooks key must name a
    /// real dispatch slot: `<op>.<pre|post>` for the phased ops, bare `show`/
    /// `list` for the reads (one phase, bare key — §6); `conf` itself dispatches
    /// no chain, so `conf.*` is not a key.
    pub(crate) fn parse(token: &str) -> io::Result<Key> {
        match token {
            "task-remote" => Ok(Key::TaskRemote),
            "task-branch" => Ok(Key::TaskBranch),
            "log-level" => Ok(Key::LogLevel),
            "show" | "list" => Ok(Key::Hook(token.to_string())),
            _ if hook_key(token) => Ok(Key::Hook(token.to_string())),
            _ => Err(io::Error::other(format!(
                "conf: unknown key '{token}' — keys: task-remote, task-branch, log-level, <op>.<pre|post>, show, list"
            ))),
        }
    }
}

/// Is `token` a phased `[hooks]` dispatch key — `<op>.<pre|post>` where `<op>`
/// is a real verb with a pre/post split (the reads dispatch a bare key and
/// `conf` is chainless, so neither phases)?
fn hook_key(token: &str) -> bool {
    let Some((op, phase)) = token.split_once('.') else {
        return false;
    };
    matches!(phase, "pre" | "post")
        && matches!(Verb::parse(op), Some(v) if !matches!(v, Verb::Show | Verb::List | Verb::Conf))
}

/// One resolved value plus the §4 layer that answered — what the dump and the
/// single-key read print.
struct Resolved {
    value: String,
    layer: String,
}

/// `bl conf [<key>] | bl conf set|append|prepend|remove <key> <value...>` —
/// dispatch on the first token: a write op routes to the sibling [`write`]
/// module, no args is the full provenance dump, one arg reads one key. Reads
/// put the value on stdout (the verb's one product) and provenance on stderr.
pub fn run(edge: &Edge, args: &[String]) -> io::Result<()> {
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    if !clone.landing().join("config").is_dir() {
        return Err(io::Error::other("no balls checkout here — run `bl prime` first"));
    }
    match args.split_first() {
        None => dump(edge, &clone),
        Some((op, rest)) if matches!(op.as_str(), "set" | "append" | "prepend" | "remove") => {
            write::run(edge, &clone, op, rest)
        }
        Some((key, [])) => read_one(edge, &clone, key),
        Some((key, _)) => Err(io::Error::other(format!(
            "conf: '{key}' takes no value on a read — values go with set/append/prepend/remove"
        ))),
    }
}

/// `bl conf <key>`: the one resolved value on stdout, its provenance on stderr.
fn read_one(edge: &Edge, clone: &CloneDir, key: &str) -> io::Result<()> {
    let r = resolve(edge, clone, &Key::parse(key)?)?;
    println!("{}", r.value);
    eprintln!("conf: {key} from {}", r.layer);
    Ok(())
}

/// `bl conf`: every resolved value + its layer, then the file paths (§4) — the
/// scalars first, then each effective `[hooks]` key in schedule order.
fn dump(edge: &Edge, clone: &CloneDir) -> io::Result<()> {
    let mut rows = Vec::new();
    for key in ["task-remote", "task-branch", "log-level"] {
        rows.push((key.to_string(), resolve(edge, clone, &Key::parse(key)?)?));
    }
    let landing = clone.landing();
    let hooks = Hooks::effective(&landing, &edge.xdg.user_config())?;
    for (key, names) in hooks.entries() {
        let value = if names.is_empty() { "(none)".into() } else { names.join(", ") };
        rows.push((key.clone(), Resolved { value, layer: hook_layer(edge, clone, key)? }));
    }
    let key_w = rows.iter().map(|(k, _)| k.len()).fold(0, usize::max) + 2;
    let val_w = rows.iter().map(|(_, r)| r.value.len()).fold(0, usize::max) + 2;
    for (key, r) in rows {
        println!("{key:<key_w$}{:<val_w$}{}", r.value, r.layer);
    }
    println!();
    println!("xdg      {}", edge.xdg.user_config().display());
    println!("landing  {}", landing.display());
    println!("store    {}", clone.store().display());
    Ok(())
}

/// Resolve one key across its §4 layers, innermost wins, naming the layer.
fn resolve(edge: &Edge, clone: &CloneDir, key: &Key) -> io::Result<Resolved> {
    let landing = clone.landing();
    match key {
        Key::TaskRemote => Ok(task_remote(edge)),
        Key::TaskBranch => scalar(edge, &landing, "tasks_branch", crate::DEFAULT_TASKS_BRANCH, None),
        Key::LogLevel => scalar(edge, &landing, "log_level", "info", edge.log_level.as_deref()),
        Key::Hook(k) => {
            let hooks = Hooks::effective(&landing, &edge.xdg.user_config())?;
            let names = hooks
                .entries()
                .find(|(key, _)| *key == k)
                .map(|(_, names)| names.join(", "))
                .filter(|joined| !joined.is_empty());
            Ok(Resolved {
                value: names.unwrap_or_else(|| "(none)".into()),
                layer: hook_layer(edge, clone, k)?,
            })
        }
    }
}

/// The store remote per the §12 ladder's DURABLE tiers (`conf` takes no
/// `--remote`): the XDG `task-remote`, else the project repo's `origin` (a
/// local `git remote get-url` — naming, not contacting, §12), else `(none)` —
/// the visible stealth read (bl-d234).
fn task_remote(edge: &Edge) -> Resolved {
    if let Some(url) = config::xdg_remote(&edge.xdg.user_config()) {
        return Resolved { value: url, layer: "xdg".into() };
    }
    match origin(&edge.invocation_path) {
        Some(url) => Resolved { value: url, layer: "origin".into() },
        None => Resolved { value: "(none)".into(), layer: "stealth".into() },
    }
}

/// `git remote get-url origin` on the PROJECT repo (the invocation path, §12) —
/// a local config read; absent origin or a non-repo path ⇒ `None`.
fn origin(project: &Path) -> Option<String> {
    let url = crate::git::run(project, &["remote", "get-url", "origin"], None).ok()?;
    Some(url.trim().to_string())
}

/// Resolve a scalar field across the §4 stack — `cli` (a flag the edge already
/// stripped) > `xdg` > `landing` > `default` — reading each layer table
/// directly so the ANSWERING layer is named, not just the folded value.
fn scalar(edge: &Edge, landing: &Path, field: &str, default: &str, cli: Option<&str>) -> io::Result<Resolved> {
    if let Some(v) = cli {
        return Ok(Resolved { value: v.to_string(), layer: "cli".into() });
    }
    let layers = [
        (config::read_layer(&edge.xdg.user_config())?, "xdg"),
        (config::read_layer(&landing.join("config").join("balls.toml"))?, "landing"),
    ];
    for (table, layer) in layers {
        if let Some(v) = table.as_ref().and_then(|t| t.get(field)).and_then(|v| v.as_str()) {
            return Ok(Resolved { value: v.to_string(), layer: layer.into() });
        }
    }
    Ok(Resolved { value: default.to_string(), layer: "default".into() })
}

/// Which layer(s) mention a `[hooks]` key — the landing `plugins.toml`, the XDG
/// overlay, or both (the §4 list compose can draw on both at once; a key in
/// neither file resolved through a directive-less default and reads `default`).
fn hook_layer(edge: &Edge, clone: &CloneDir, key: &str) -> io::Result<String> {
    let landing = hooks_mentions(&clone.landing().join("config").join("plugins.toml"), key)?;
    let xdg = hooks_mentions(&edge.xdg.user_config().with_file_name("plugins.toml"), key)?;
    Ok(match (landing, xdg) {
        (true, true) => "landing+xdg".into(),
        (true, false) => "landing".into(),
        (false, true) => "xdg".into(),
        (false, false) => "default".into(),
    })
}

/// Does this `plugins.toml` layer's `[hooks]` table mention `key` — bare or via
/// any §4 compose directive? An absent file mentions nothing.
fn hooks_mentions(path: &Path, key: &str) -> io::Result<bool> {
    let Some(root) = config::read_layer(path)? else {
        return Ok(false);
    };
    let Some(toml::Value::Table(hooks)) = root.get("hooks") else {
        return Ok(false);
    };
    Ok(hooks.contains_key(key)
        || ["_prepend", "_append", "_ban"].iter().any(|s| hooks.contains_key(&format!("{key}{s}"))))
}

#[cfg(test)]
#[path = "conf_tests.rs"]
mod tests;
