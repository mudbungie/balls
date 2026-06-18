//! §16 `bl import` — the write inverse of the bedrock read (`show --json`).
//!
//! `show --json` reads the lossless record OUT; `import` writes the same shape
//! BACK IN, through the real store and the real `task.rs` serializer: id and
//! timestamps verbatim, NO mint / NO stamp / NO gate. It is not
//! migration-specific — "reproduce a task that already has an identity" is the
//! shared primitive behind federation join, restore-from-backup, and
//! clone-seeding — and it is deliberately not `create --id`: `create` mints a
//! NEW identity (and rightly refuses a foreign one); `import`'s ids are
//! authoritative at the source.
//!
//! **Collision semantics — refuse, don't guess (§16):** an id that already
//! lives in the store (or repeats within the stream) aborts the WHOLE stream
//! before anything is written, exit nonzero naming the id(s). Skip would lie
//! (a "restored" ball silently isn't), replace would destroy local state on no
//! explicit signal — both guesses; the caller composes the remedy (`bl close`
//! the holder, or filter the stream) with the verbs that already exist, so no
//! flag is added.
//!
//! `--legacy[=REF]` is the cutover button: pure orchestration over the §16
//! read shim + this primitive — exactly `bl list --legacy --json | bl import`
//! in-process, plus the epic reciprocal-edge pass wired through the ordinary
//! `update --needs` machinery. It carries no policy the pipe can't express.

use std::collections::{BTreeMap, HashSet};
use std::io::{self, Read};
use std::path::Path;

use crate::edge::Edge;
use crate::lifecycle::BaseChange;
use crate::message::Message;
use crate::mutate::{self, Op};
use crate::reads::legacy;
use crate::task::Task;
use crate::taskfile::{exists, write_task};
use crate::verb::Verb;
use crate::wire::Command;

#[path = "import_stream.rs"]
mod stream;

/// Run `bl import`: parse flags, source the records — the verbatim bedrock
/// JSON on `input` (one object, an array, or a concatenated stream: whatever
/// `show --json`/`list --json` emit), or the §16 legacy projection under
/// `--legacy` — and seal them in. `input` is the caller's stdin, injected so
/// the verb is testable and never read on the `--legacy` path.
pub fn run(edge: &Edge, input: &mut dyn Read, args: &[String]) -> io::Result<()> {
    let flags = parse(args, &edge.default_actor)?;
    if let Some(spec) = &flags.legacy {
        return button(edge, &flags, spec);
    }
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    ingest(edge, &flags, stream::records(&text)?)
}

/// `import`'s flags: identity, narration, the §12 remote override pair, and
/// the `--legacy[=REF]` button. No positionals — records ride stdin.
#[derive(Debug)]
struct Flags {
    actor: String,
    message: Option<String>,
    remote: Option<String>,
    legacy: Option<String>,
}

/// Parse `import`'s argv. The flag vocabulary is deliberately the mutating
/// verbs' (`--as`/`-m`/`--remote`/`--center`) plus the §16 `--legacy` spelling
/// shared with the read verbs ([`legacy::flag`]).
fn parse(args: &[String], default_actor: &str) -> io::Result<Flags> {
    let mut f = Flags { actor: default_actor.to_string(), message: None, remote: None, legacy: None };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--as" => f.actor = value(&mut args, "--as")?,
            "-m" | "--message" => f.message = Some(value(&mut args, "-m")?),
            flag @ ("--remote" | "--center") => {
                crate::checkout::apply_remote(&mut f.remote, flag, value(&mut args, flag)?);
            }
            a if legacy::flag(a).is_some() => f.legacy = legacy::flag(a),
            other => {
                return Err(crate::usage(format!(
                    "import: unexpected argument '{other}' — records ride stdin"
                )))
            }
        }
    }
    Ok(f)
}

/// The value following a value-taking flag, or a "needs a value" error.
fn value(args: &mut std::slice::Iter<'_, String>, flag: &str) -> io::Result<String> {
    args.next().cloned().ok_or_else(|| crate::usage(format!("import: {flag} needs a value")))
}

/// Seal `balls` into the store as ONE op through the shared engine wiring
/// ([`mutate::seal_op`]) — one commit, all-or-nothing, so the refuse-the-stream
/// collision check and the §14 rollback give the same guarantee: a failed
/// import imports nothing. Confirmation on stderr; stdout stays silent (§9 —
/// the caller supplied every id, so the verb has no product to print).
fn ingest(edge: &Edge, flags: &Flags, balls: Vec<(String, Task)>) -> io::Result<()> {
    let n = balls.len();
    let base = Ingest { balls, actor: flags.actor.clone(), message: flags.message.clone() };
    let op = Op {
        actor: flags.actor.clone(),
        remote: flags.remote.clone(),
        command: Command { op: Verb::Import.token().to_string(), body_change: None },
    };
    mutate::seal_op(edge, Verb::Import, &op, &base, None)?;
    eprintln!("import {n} ball{}", if n == 1 { "" } else { "s" });
    Ok(())
}

/// The bulk [`BaseChange`]: write every record verbatim through the §3
/// serializer ([`write_task`]). No enforce gate runs — the records are
/// reproductions, not transitions — and nothing is restamped.
struct Ingest {
    balls: Vec<(String, Task)>,
    actor: String,
    message: Option<String>,
}

impl BaseChange for Ingest {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        // Collision = an id the store already holds OR one repeated in the
        // stream: refuse the WHOLE stream before writing anything.
        let mut seen = HashSet::new();
        let held: Vec<&str> = self
            .balls
            .iter()
            .map(|(id, _)| id.as_str())
            .filter(|id| !seen.insert(*id) || exists(dir, id))
            .collect();
        if !held.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "import: id(s) already held: {} — refusing the whole stream, nothing imported (retire the holder or strip the record, then re-run)",
                    held.join(", ")
                ),
            ));
        }
        for (id, task) in &self.balls {
            write_task(dir, id, task)?;
        }
        Ok(())
    }

    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        // A bulk op names no single ball, so it takes §5's id-less form (like
        // a checkout-scoped op): the subject counts, the file diff is the record.
        let n = self.balls.len();
        Message {
            verb: Verb::Import,
            actor: self.actor.clone(),
            id: None,
            subject: format!("import {n} ball{}", if n == 1 { "" } else { "s" }),
            body: self.message.clone(),
        }
        .render()
    }

    fn narrated(&self) -> bool {
        // bl-cf93's loud-loss rule: an empty stream with `-m` would converge the
        // seal and silently drop the note — abort instead (≥1 record never
        // converges, the collision check refuses every existing id first).
        self.message.is_some()
    }
}

/// `bl import --legacy[=REF]` — the §16 cutover button: read the legacy store
/// through the SAME projection `bl list --legacy` renders (the pipe,
/// in-process), import every live ball, then mint the epic reciprocal edges —
/// each live child claim-blocks its live parent — as ordinary `update --needs`
/// ops, one per parent. Legacy derived "an epic waits on its children" from
/// status; greenfield `parent:` is containment-only (§10), so the edge is a
/// reconstruction, and it rides the real edge machinery (real ops, real
/// `updated` stamps) rather than a second transform path.
fn button(edge: &Edge, flags: &Flags, spec: &str) -> io::Result<()> {
    let balls = legacy::balls(&edge.invocation_path, spec)?;
    // The projection already nulled dangling parents (§16), so every surviving
    // `parent` is in the imported live set.
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, task) in &balls {
        if let Some(parent) = &task.parent {
            edges.entry(parent.clone()).or_default().push(id.clone());
        }
    }
    ingest(edge, flags, balls)?;
    for (parent, children) in edges {
        let mut args = vec![parent];
        for child in children {
            args.push("--needs".into());
            args.push(child);
        }
        args.extend(["--as".into(), flags.actor.clone()]);
        if let Some(remote) = &flags.remote {
            args.extend(["--remote".into(), remote.clone()]);
        }
        mutate::run(edge, Verb::Update, &args)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
