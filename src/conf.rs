//! §4/§9 `bl conf` — local config CRUD with provenance (bl-c2de).
//!
//! The §4 "by you" path made a surface: READ resolves every config value across
//! the §4 layers and reports WHICH layer answered plus the file paths behind
//! them (the "where are my files / what remote am I actually using" answer —
//! stealth shows as `task-remote (none)`, closing the bl-d234 invisible-stealth
//! gap); WRITE is scope-keyed CRUD where the KEY implies its canonical home
//! (no `--scope` flag): `task-remote` routes by VALUE — a URL is per-machine
//! (XDG `config.toml`; a URL must not travel on `install`), the stealth
//! sentinel `none` is per-checkout POLICY on the landing `balls.toml` (§12,
//! bl-9df0) — while `task-branch`/`log-level` live on the landing `balls.toml`
//! and the `[hooks]` schedule keys on the landing `plugins.toml` ([`write`],
//! the sibling module).
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

use crate::edge::Edge;
use crate::hooks::Hooks;
use crate::layout::CloneDir;
use crate::verb::Verb;
use std::io;

#[path = "conf_write.rs"]
mod write;
pub(crate) use write::declare_stealth;

// The §4 layer resolution + provenance engine lives in a sibling; `conf` keeps
// the key namespace and the read/dump dispatch.
#[path = "conf_resolve.rs"]
mod prov;

/// The §4 key namespace `conf` reads and writes. A key carries its canonical
/// home (the table in §4): the three scalars name built-in fields, [`Key::Hook`]
/// is any `[hooks]` schedule key — `<op>.<pre|post>` for the phased ops, or the
/// bare `show`/`list` a read op dispatches (§6).
#[derive(Debug)]
pub(crate) enum Key {
    /// The store remote (§12). Its home routes by VALUE: a URL is per-machine
    /// (the XDG `remote` field — a URL must not travel on `install`, §4); the
    /// stealth sentinel `none` is the per-checkout landing `task_remote` policy
    /// rung (bl-9df0).
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
    let r = prov::resolve(edge, clone, &Key::parse(key)?)?;
    println!("{}", r.value);
    eprintln!("conf: {key} from {}", r.layer);
    Ok(())
}

/// `bl conf`: every resolved value + its layer, then the file paths (§4) — the
/// scalars first, then each effective `[hooks]` key in schedule order.
fn dump(edge: &Edge, clone: &CloneDir) -> io::Result<()> {
    let mut rows = Vec::new();
    for key in ["task-remote", "task-branch", "log-level"] {
        rows.push((key.to_string(), prov::resolve(edge, clone, &Key::parse(key)?)?));
    }
    let landing = clone.landing();
    let hooks = Hooks::effective(&landing, &edge.xdg.user_config())?;
    for (key, names) in hooks.entries() {
        let value = if names.is_empty() { "(none)".into() } else { names.join(", ") };
        rows.push((key.clone(), prov::Resolved { value, layer: prov::hook_layer(edge, clone, key)? }));
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

#[cfg(test)]
#[path = "conf_tests.rs"]
mod tests;
