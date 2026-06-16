//! §5 commit-message protocol.
//!
//! Every change-attempt commit is `subject / body / trailer-block`, where the
//! trailer block is a **standard git trailer paragraph** — the last
//! blank-line-separated paragraph of `key: value` lines. balls owns neither end
//! of the grammar: [`Message::render`] appends its trailers with
//! `git interpret-trailers --trailer`, and [`parse`] reads them back with
//! `git interpret-trailers --parse`. There is deliberately **no hand-rolled
//! parser** (§5) — git decides what is and isn't a trailer, so balls trailers
//! coexist with `Co-Authored-By:` and anything else the body already carries.
//!
//! Two protocol rules fall out of that delegation for free:
//!
//! - **`bl-` is reserved to core.** balls is the sole author of the trailer
//!   block's machine keys — it appends `bl-protocol`/`bl-op`/`bl-actor` (and
//!   `bl-id` on per-task ops) at seal time. Plugins have no return channel (§7):
//!   they edit the change worktree, never the commit message, so they
//!   structurally *cannot* emit a `bl-*` trailer. A plugin's own keys ride
//!   self-prefixed (`jira-id`, `github-url`) in the body.
//! - **Unknown keys are never dropped.** `interpret-trailers` preserves any
//!   trailer the body already holds, and [`parse`] groups a repeated key into a
//!   value list (git-native; no comma-splitting). Those non-core keys flow into
//!   [`Metadata`] and out to plugins on the post wire (§7).

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::process::{Command, Stdio};

use crate::verb::Verb;

/// The §5 protocol version every balls commit declares as `bl-protocol`.
pub const PROTOCOL: u32 = 1;

/// Trailers parsed from a commit's block: each key mapped to its value list, so
/// a repeated key (`bl-tag: a` / `bl-tag: b`) is a two-element `Vec` (§5). This
/// is the `metadata` balls forwards to plugins on the post wire (§7).
pub type Metadata = BTreeMap<String, Vec<String>>;

/// A commit balls is about to seal: a `subject` (always the ball title — there
/// is no override, §5), an optional body (the `-m` narration),
/// and the op/actor/id that fix its core trailers. `id` is `Some` for a
/// per-task op (`create`/`claim`/`unclaim`/`update`/`close`) and `None`
/// for a checkout-scoped op (`prime`/`sync`/`install`) that names no single
/// ball (§5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub verb: Verb,
    pub actor: String,
    pub id: Option<String>,
    pub subject: String,
    pub body: Option<String>,
}

impl Message {
    /// A checkout-scoped seal's message (§5): `prime`/`install`/`conf` name no
    /// single ball, so `bl-id` is absent — but the other three core trailers
    /// (`bl-protocol`/`bl-op`/`bl-actor`) ride every balls commit alike
    /// (bl-1d9b). The subject is the op's own `balls: …` line (there is no ball
    /// title to carry), and there is no `-m` narration on these ops.
    pub fn checkout(verb: Verb, actor: &str, subject: String) -> Message {
        Message { verb, actor: actor.to_string(), id: None, subject, body: None }
    }

    /// Render to the full `subject / body / trailer-block` text, with the core
    /// `bl-*` trailers appended via `git interpret-trailers`. Any trailer the
    /// body already carries (a plugin's self-prefixed key) is merged into the
    /// same block and preserved.
    pub fn render(&self) -> io::Result<String> {
        let mut input = self.subject.clone();
        if let Some(body) = &self.body {
            input.push_str("\n\n");
            input.push_str(body);
        }

        let mut trailers = vec![
            format!("bl-protocol={PROTOCOL}"),
            format!("bl-op={}", self.verb.token()),
        ];
        if let Some(id) = &self.id {
            trailers.push(format!("bl-id={id}"));
        }
        trailers.push(format!("bl-actor={}", self.actor));

        // `--if-exists add` keeps a repeated key as a list rather than letting
        // the default neighbor-dedup collapse it.
        let mut args = vec!["interpret-trailers", "--if-exists", "add"];
        for trailer in &trailers {
            args.push("--trailer");
            args.push(trailer);
        }
        run_git(&args, &input)
    }
}

/// Parse a commit message's trailer block into [`Metadata`], grouping a
/// repeated key into its value list (§5). git decides the block boundary
/// (`--parse` unfolds and emits one normalized `key: value` per line); balls
/// only splits each line at its separating colon.
pub fn parse(message: &str) -> io::Result<Metadata> {
    let trailers = run_git(&["interpret-trailers", "--parse"], message)?;
    let mut metadata = Metadata::new();
    for (key, value) in trailers.lines().filter_map(|line| line.split_once(':')) {
        metadata
            .entry(key.trim().to_string())
            .or_default()
            .push(value.trim().to_string());
    }
    Ok(metadata)
}

/// Feed `stdin` to `git <args>` and return its stdout. The single git-invocation
/// site for both render and parse.
fn run_git(args: &[&str], stdin: &str) -> io::Result<String> {
    let mut child = Command::new("git")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("stdin was configured as a pipe")
        .write_all(stdin.as_bytes())?;
    let out = child.wait_with_output()?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
