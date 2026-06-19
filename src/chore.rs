//! `bl-chore` — the guarded close-gate mint at claim (design bl-3df3).
//!
//! An opt-in, first-party plugin: at `claim.post`, for the just-claimed task, it
//! mints one close-gate child per configured chore ("Run the test suite", ...) so
//! the claiming agent must discharge them before `bl close` succeeds. It is the
//! CREATE side only — a human (or a resolver plugin) closes the gate; bl-chore
//! never resolves it (§10). Two guards keep it sound:
//!
//! - **tag-skip** (always-on, structural): if the claimed task carries bl-chore's
//!   own tag, bail — claiming a chore must not mint a chore-of-a-chore. A chore is
//!   a LEAF, so the epic-skip has-children check would not catch it. Read off the
//!   §7 wire (`previous_state.tags`), no store query.
//! - **epic-skip** (a knob, default-on, in the plugin's own config): if the
//!   claimed task already has any live child, bail — keeps epics clutter-free AND
//!   gives idempotency for free (a reclaim finds the chores it minted before).
//!   Children are not on the wire, so this is the one store query (`bl list`).
//!
//! The plugin OWNS the whole `bl create` command (data-not-shell, design bl-3df3):
//! the config contributes only a title (± body/priority), and the gate edge +
//! the recursion-break tag are always injected here — so a caller can never
//! forget the tag, and getopt's `--` is always placed by us. There is NO rollback
//! handler: each shelled `bl create` seals+pushes independently, so a minted gate
//! correctly persists to gate whoever next holds the task, and epic-skip de-dups
//! a reclaim. All policy lives here behind the [`Bl`] seam ([`crate::chore_cli`]
//! is the real one); the binary edge only adapts the process boundary.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The §6 self-description, emitted on `bl-chore protocol`. Declares the one op
/// it handles; the `claim.post` WIRING is config (`bl conf prepend claim.post
/// bl-chore`), never the binary (§6) — balls reads this only to validate a bind.
pub const PROTOCOL_JSON: &str = r#"{"protocol":[1],"ops":["claim"]}"#;

/// The slice of the §7 wire bl-chore reads. Output-only [`crate::wire`] is the
/// core's side; this is the receiving end, so it owns its own input type and
/// serde drops every field it does not name — keeping it stable as the wire grows.
#[derive(Deserialize)]
struct Wire {
    #[serde(default)]
    binding: WireBinding,
    /// §5 trailers (incl. the now-sealed `bl-id`); absent on a `pre` wire.
    #[serde(default)]
    metadata: BTreeMap<String, Vec<String>>,
    /// The op-start ball — what `pre` saw as `current_state` (§7). Carries the
    /// claimed task's tags for tag-skip; `null` on create.
    #[serde(default)]
    previous_state: Option<WireTask>,
    /// `Some("pre"|"post")` only on a rollback call — bl-chore no-ops it.
    #[serde(default)]
    rolling_back: Option<String>,
    /// The invoking identity (`--as`), passed through to the minted children.
    #[serde(default)]
    actor: String,
}

/// `landing` (where this plugin's committed config lives) and `invocation_path`
/// (the project repo root — the cwd for shelling `bl`). Both default empty so a
/// partial/stealth wire still deserializes (the guards short-circuit first).
#[derive(Deserialize, Default)]
struct WireBinding {
    #[serde(default)]
    landing: String,
    #[serde(default)]
    invocation_path: String,
}

/// Just the claimed task's tags — the only field tag-skip needs.
#[derive(Deserialize)]
struct WireTask {
    #[serde(default)]
    tags: Vec<String>,
}

/// bl-chore's own config (`<landing>/config/plugins/<name>/chores.toml`) — a list
/// of declarative chore specs plus the epic-skip knob. Lives in the plugin's own
/// territory (balls never reads it, §4 severability).
#[derive(Deserialize)]
struct Config {
    /// Default-ON: bail when the claimed task already has children.
    #[serde(default = "enabled")]
    epic_skip: bool,
    /// The chores to mint, in order. `[[chore]]` array-of-tables.
    #[serde(default)]
    chore: Vec<ChoreSpec>,
}

/// One chore: a title (required) plus optional declared body/priority — never
/// flags or shell. bl-chore builds the whole `bl create` argv from these.
#[derive(Deserialize)]
struct ChoreSpec {
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
}

/// The epic-skip default — a missing `epic_skip` key is ON (the conservative call:
/// a leaf you gave one real subtask gets no chores; override per-repo in config).
fn enabled() -> bool {
    true
}

/// The `bl`-invocation seam: run `bl <argv>` with `cwd`, returning captured
/// stdout on success. The production impl shells the binary ([`crate::chore_cli`]);
/// tests inject a fake. This is the only side effect bl-chore has.
pub trait Bl {
    /// Run `bl <argv>` in `cwd`; `Ok(stdout)` on a zero exit, `Err` otherwise.
    /// `argv` owns its elements (`&[String]`, not the `&[&str]` of the core git
    /// seams) because [`render_create`] interleaves owned dynamic values
    /// (title/body/actor) with literals — there is no borrow to lend.
    fn run(&self, cwd: &Path, argv: &[String]) -> io::Result<String>;
}

/// Dispatch one bl-chore invocation. `op`/`phase` are argv; `plugin` is the
/// schedule name (BALLS_PLUGIN_NAME — the recursion-break tag AND the config
/// territory both derive from it); `stdin` is the §7 wire; `bl` is the seam.
///
/// Only the `claim.post` forward pass mints; every other (op, phase) and any
/// rollback is a no-op. A malformed payload or a failed `bl create` is an error
/// (aborts the claim); a guard firing, or nothing to do, is a clean `Ok(())`.
pub fn run(op: &str, phase: &str, plugin: &str, stdin: &str, bl: &dyn Bl) -> io::Result<()> {
    let wire: Wire = serde_json::from_str(stdin).map_err(io::Error::other)?;
    if op != "claim" || phase != "post" || wire.rolling_back.is_some() {
        return Ok(());
    }
    // tag-skip (always, off the wire) — break chore-of-a-chore before any query.
    // A chore is a LEAF, so epic-skip's has-children check would not catch it.
    if wire.previous_state.as_ref().is_some_and(|s| s.tags.iter().any(|t| t == plugin)) {
        return Ok(());
    }
    let config = load_config(&config_path(&wire.binding.landing, plugin))?;
    if config.chore.is_empty() {
        return Ok(());
    }
    let id = claimed_id(&wire.metadata)?;
    let cwd = Path::new(&wire.binding.invocation_path);
    // epic-skip (the one store query — children are emergent, not on the wire).
    if config.epic_skip && has_children(&bl.run(cwd, &["list".to_string(), "--json".to_string()])?, id)? {
        return Ok(());
    }
    for spec in &config.chore {
        bl.run(cwd, &render_create(spec, id, plugin, &wire.actor))?;
    }
    Ok(())
}

/// The §5 `bl-id` of the just-claimed task — the one field that names what to
/// gate. Absent is a contract violation on a `claim.post` wire (§7).
fn claimed_id(metadata: &BTreeMap<String, Vec<String>>) -> io::Result<&str> {
    metadata
        .get("bl-id")
        .and_then(|v| v.first())
        .map(String::as_str)
        .ok_or_else(|| io::Error::other("claim.post payload carries no bl-id (§7)"))
}

/// `<landing>/config/plugins/<plugin>/chores.toml` — the plugin's own committed
/// config territory on the landing (§2/§4).
fn config_path(landing: &str, plugin: &str) -> PathBuf {
    Path::new(landing).join("config").join("plugins").join(plugin).join("chores.toml")
}

/// Read + parse the chore config; an ABSENT file is the present-but-empty config
/// (mint nothing — opting the plugin in without writing chores is a valid no-op),
/// routed through the SAME serde defaults so `epic_skip`-on lives in one place.
fn load_config(path: &Path) -> io::Result<Config> {
    match fs::read_to_string(path) {
        Ok(s) => toml::from_str(&s).map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => toml::from_str("").map_err(io::Error::other),
        Err(e) => Err(e),
    }
}

/// Whether any live task in the `bl list --json` array names `parent` as its
/// parent — the epic-skip predicate (live set only, so a fully-discharged task
/// re-mints, which is correct: its next holder is re-gated).
fn has_children(list_json: &str, parent: &str) -> io::Result<bool> {
    let items: Vec<ListItem> = serde_json::from_str(list_json).map_err(io::Error::other)?;
    Ok(items.iter().any(|it| it.parent.as_deref() == Some(parent)))
}

/// Just the `parent` pointer of a `bl list --json` row.
#[derive(Deserialize)]
struct ListItem {
    #[serde(default)]
    parent: Option<String>,
}

/// Build the full `bl create` argv for one chore: the gate edge (`--parent <id>
/// --blocks close`) and the recursion-break tag (`-t <plugin>`) are always
/// injected; the spec's optional priority/body ride before the `--`; the title
/// is the lone trailing positional, safe from flag-hijack (design bl-3df3).
fn render_create(spec: &ChoreSpec, parent: &str, tag: &str, actor: &str) -> Vec<String> {
    let mut argv = vec![
        "create".to_string(),
        "--parent".to_string(),
        parent.to_string(),
        "--blocks".to_string(),
        "close".to_string(),
        "-t".to_string(),
        tag.to_string(),
        "--as".to_string(),
        actor.to_string(),
    ];
    if let Some(p) = spec.priority {
        argv.push("-p".to_string());
        argv.push(p.to_string());
    }
    if let Some(body) = &spec.body {
        argv.push("--body".to_string());
        argv.push(body.clone());
    }
    argv.push("--".to_string());
    argv.push(spec.title.clone());
    argv
}

#[cfg(test)]
#[path = "chore_tests.rs"]
mod tests;
