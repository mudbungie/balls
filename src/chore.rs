//! `bl-chore` — the guarded close-gate mint at claim (design bl-3df3).
//!
//! An opt-in, first-party plugin: at `claim.pre`, for the task being claimed, it
//! mints one close-gate child per configured chore ("Run the test suite", ...) so
//! the claiming agent must discharge them before `bl close` succeeds. It is the
//! CREATE side only — a human (or a resolver plugin) closes the gate; bl-chore
//! never resolves it (§10). Two guards keep it sound:
//!
//! - **tag-skip** (always-on, structural): if the claimed task carries bl-chore's
//!   own tag, bail — claiming a chore must not mint a chore-of-a-chore. A chore is
//!   a LEAF, so the epic-skip has-children check would not catch it. Read off the
//!   §7 wire (`current_state.tags`), no store query.
//! - **epic-skip** (a knob, default-on, in the plugin's own config): if the
//!   claimed task already has any live child, bail — keeps epics clutter-free AND
//!   gives idempotency for free (a reclaim finds the chores it minted before).
//!
//! **THE MINT IS A FILE WRITE, NOT AN OP (bl-1da3).** It used to shell `bl
//! create` once per chore. §14's appendix — rollback for effects whose binding
//! artifact lives in an EXTERNAL system — was stretched to cover that, calling
//! balls "the external tracker that assigns its own id". Balls is not external to
//! itself. Core CAN reach the store: it is the change worktree this plugin is
//! invoked in, and `pre` is the sanctioned door (§8 step 2, "pre modifiers …
//! edit the shared worktree"; §8.3, "a `pre` plugin edits the SHARED change
//! worktree, so it can also touch a SIBLING `tasks/*.md`"). So the mint is two
//! ordinary writes into that worktree — `tasks/<child>.md`, and the `{id, on:
//! close}` blocker onto the parent's file, which is already there because
//! `base.stage` ran before this phase.
//!
//! What that DELETES is the point: the nested op and its own commit point, the
//! `Bl` shell seam, the scratch record that carried ids across a process
//! boundary, the rollback that took them back down, the mid-list inline unwind,
//! and the `close.post` sweep of the record. **There is no rollback because
//! there is no separate effect to undo** — the children live in the claim's own
//! change worktree, so an aborted claim discards them with it, and the §14
//! appendix stops applying rather than being satisfied.
//!
//! Nothing derives a clock or an identity twice: the op instant is read off the
//! parent's freshly-staged `updated`, and the child inherits the parent's
//! `root_commit` (bl-1ce7) rather than re-asking git.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::id::IdScheme;
use crate::task::{Blocker, On, Task};
use crate::taskfile::{add_blocker, read_task, task_ids, write_task};

/// The §6 self-description, emitted on `bl-chore protocol`. ONE op now: the mint
/// is part of the claim's own atom, so there is no `close` record to retire and
/// no rollback to declare. The WIRING (`bl conf prepend claim.pre bl-chore`) is
/// config, never the binary (§6) — balls reads this only to validate a bind.
pub const PROTOCOL_JSON: &str = r#"{"protocol":[1],"ops":["claim"]}"#;

/// The slice of the §7 wire bl-chore reads. Output-only [`crate::wire`] is the
/// core's side; this is the receiving end, so it owns its own input type and
/// serde drops every field it does not name — keeping it stable as the wire grows.
#[derive(Deserialize)]
struct Wire {
    #[serde(default)]
    binding: WireBinding,
    /// The op's ball — on EVERY payload, `pre` included (§7 `Command::id`), which
    /// is what lets the mint run before the seal exists. The old `post` mint read
    /// the §5 `bl-id` trailer instead; that trailer is not written yet here.
    #[serde(default)]
    command: WireCommand,
    /// The op-start ball. On a `pre` wire it is `current_state` (`previous_state`
    /// is the `post` spelling of the same thing, §7). Carries the tags tag-skip
    /// reads.
    #[serde(default)]
    current_state: Option<WireTask>,
    /// `Some("pre"|"post")` only on a rollback call. Nothing to unwind — the
    /// children are in the change worktree core is about to discard — so this
    /// only distinguishes the forward pass from the unwind.
    #[serde(default)]
    rolling_back: Option<String>,
}

/// `landing` — where this plugin's committed config lives. The project repo root
/// is no longer needed: nothing is shelled, and the worktree the mint writes is
/// this process's cwd. Defaults empty so a partial/stealth wire still
/// deserializes (the guards short-circuit first).
#[derive(Deserialize, Default)]
struct WireBinding {
    #[serde(default)]
    landing: String,
}

/// Just the ball the op is about.
#[derive(Deserialize, Default)]
struct WireCommand {
    #[serde(default)]
    id: Option<String>,
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
/// flags or shell. The gate edge and the recursion-break tag are this module's,
/// so a caller can never forget them.
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

/// Dispatch one bl-chore invocation. `op`/`phase` are argv; `plugin` is the
/// schedule name (`BALLS_PLUGIN_NAME` — the recursion-break tag AND the config
/// territory both derive from it); `dir` is the CHANGE WORKTREE (the process cwd
/// core invokes a hook in), which is both what the mint writes and what
/// epic-skip reads; `stdin` is the §7 wire.
///
/// Only the `claim.pre` forward pass does anything. A ROLLBACK is a no-op by
/// construction, not by care: whatever this wrote is in the worktree core
/// discards. A malformed payload or a failed write is an error (aborts the
/// claim); a guard firing, or nothing to do, is a clean `Ok(())`.
pub fn run(op: &str, phase: &str, plugin: &str, dir: &Path, stdin: &str) -> io::Result<()> {
    let wire: Wire = serde_json::from_str(stdin).map_err(io::Error::other)?;
    if op != "claim" || phase != "pre" || wire.rolling_back.is_some() {
        return Ok(());
    }
    // tag-skip (always, off the wire) — break chore-of-a-chore before any read.
    // A chore is a LEAF, so epic-skip's has-children check would not catch it.
    if wire.current_state.as_ref().is_some_and(|s| s.tags.iter().any(|t| t == plugin)) {
        return Ok(());
    }
    let config = load_config(&config_path(&wire.binding.landing, plugin))?;
    if config.chore.is_empty() {
        return Ok(());
    }
    let id = wire.command.id.as_deref().ok_or_else(|| io::Error::other("claim.pre payload names no ball (§7 command.id)"))?;
    if config.epic_skip && has_children(dir, id)? {
        return Ok(());
    }
    mint(dir, id, plugin, &config.chore)
}

/// Write one `tasks/<child>.md` per chore and hang each on the parent as a
/// close-gate blocker. Every id is re-rolled off the ids ALREADY in the worktree,
/// so the children of one claim cannot collide with each other or with a sibling
/// `pre` plugin's fresh write (§ id generation — the scheme is public and the
/// live set is right here).
///
/// The parent is read once, for the two facts a child inherits rather than
/// re-derives: the op instant (`updated`, stamped by `base.stage` moments ago —
/// a second clock would be a second answer) and `root_commit`, so a chore is
/// bound to the same repo as the ball it gates (bl-1ce7) without asking git.
fn mint(dir: &Path, parent_id: &str, tag: &str, chores: &[ChoreSpec]) -> io::Result<()> {
    let parent = read_task(dir, parent_id)?;
    let scheme = IdScheme::default();
    for spec in chores {
        let child_id = scheme.mint(&task_ids(dir)?)?;
        let child = Task {
            title: spec.title.clone(),
            created: parent.updated,
            updated: parent.updated,
            parent: Some(parent_id.to_string()),
            priority: spec.priority,
            tags: vec![tag.to_string()],
            root_commit: parent.root_commit.clone(),
            body: spec.body.clone().unwrap_or_default(),
            ..Task::default()
        };
        write_task(dir, &child_id, &child)?;
        add_blocker(dir, parent_id, Blocker { id: child_id, on: On::Close }, parent.updated)?;
    }
    Ok(())
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

/// Whether any ball in the worktree names `parent` as its parent — the epic-skip
/// predicate. This was the plugin's one store query (`bl list --json`, a second
/// nested op); the change worktree is the same live set, one directory read away,
/// so the query had no reason to leave the process.
fn has_children(dir: &Path, parent: &str) -> io::Result<bool> {
    for id in task_ids(dir)? {
        if read_task(dir, &id)?.parent.as_deref() == Some(parent) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "chore_tests.rs"]
mod tests;
