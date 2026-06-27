//! §4 config resolution with provenance — resolve one [`Key`] across the §4
//! layers (innermost wins) and NAME the layer that answered. Lifted from
//! [`super`] so the `conf` file stays the key namespace + the read/dump dispatch;
//! this is the "which layer / what file" engine the dump and single-key read read.

use std::io;
use std::path::Path;

use crate::config;
use crate::edge::Edge;
use crate::hooks::Hooks;
use crate::layout::CloneDir;

use super::Key;

/// One resolved value plus the §4 layer that answered — what the dump and the
/// single-key read print.
pub(super) struct Resolved {
    pub(super) value: String,
    pub(super) layer: String,
}

/// Resolve one key across its §4 layers, innermost wins, naming the layer.
pub(super) fn resolve(edge: &Edge, clone: &CloneDir, key: &Key) -> io::Result<Resolved> {
    let landing = clone.landing();
    match key {
        Key::TaskRemote => task_remote(edge, &landing, &clone.binding()),
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
/// `--remote`), through the SAME [`config::remote_ladder`] the ops bind with:
/// the landing `task_remote` policy (declared stealth reads `(none)` from
/// `landing` — bl-9df0), else this clone's `binding` remote, else the legacy
/// global XDG remote, else the project repo's `origin` (a local `git remote
/// get-url` — naming, not contacting, §12), else `(none)` from `stealth` —
/// circumstantial, nothing set. A non-stealth URL is labelled by its tier:
/// `binding` (per-clone, this checkout's own) vs `xdg (global)` (per-machine,
/// shared by every repo — the bl-d081 disambiguation, so a global value is told
/// apart from a per-clone one). Three distinct no-remote readouts (bl-d234):
/// declared, unset-with-origin, unset-without.
fn task_remote(edge: &Edge, landing: &Path, binding: &Path) -> io::Result<Resolved> {
    let (remote, declared) = config::remote_ladder(None, landing, binding, &edge.xdg.user_config())?;
    if declared {
        return Ok(Resolved { value: "(none)".into(), layer: "landing".into() });
    }
    if let Some(url) = remote {
        let layer = if config::binding_remote(binding).is_some() { "binding" } else { "xdg (global)" };
        return Ok(Resolved { value: url, layer: layer.into() });
    }
    Ok(match origin(&edge.invocation_path) {
        Some(url) => Resolved { value: url, layer: "origin".into() },
        None => Resolved { value: "(none)".into(), layer: "stealth".into() },
    })
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
pub(super) fn hook_layer(edge: &Edge, clone: &CloneDir, key: &str) -> io::Result<String> {
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
