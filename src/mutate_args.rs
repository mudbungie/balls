//! §9 front-door argv parsing — the verb-agnostic [`Flags`] vocabulary and the
//! one [`parse`] over it. Split from [`crate::mutate`] so the dispatch there
//! stays orchestration; what a flag MEANS per verb stays with the verbs (the
//! `mutate_build` guards), this is purely how argv becomes [`Flags`]. All flag
//! parsing is core — plugins are hook binaries and never extend the parser
//! (§10).

use std::io;

use super::other;

/// The parsed front-door flags + positionals, verb-agnostic. The per-verb
/// `base_change` validates which it accepts. `message` is the `-m` §5 commit
/// narration (every verb); `body` is the `--body` ball markdown body
/// (create/update). EVERY ball field is overwriteable on `update` — there is no
/// create-only split: `title`/`parent`/`priority`/`tags`/extras set, and the
/// `--no-*` family clears (`no_parent`/`no_priority` clear the scalar,
/// `no_tags`/`no_needs` drop a member, a `key=` empty extra removes it). Only
/// `blocks` (a reciprocal edge on ANOTHER task) stays create-only.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Flags {
    pub actor: String,
    pub message: Option<String>,
    pub body: Option<String>,
    pub title: Option<String>,
    pub parent: Option<String>,
    pub no_parent: bool,
    /// `--subtask-of E` (§10): the everyday subtask spelling — `--parent E
    /// --blocks close` in one word, the intent named by the flag so the
    /// close-gate cannot be silently forgotten. Create-only (it carries the
    /// reciprocal edge); mutually exclusive with `--parent`.
    pub subtask_of: Option<String>,
    pub blocks: Vec<String>,
    pub needs: Vec<String>,
    pub no_needs: Vec<String>,
    pub priority: Option<i64>,
    pub no_priority: bool,
    pub tags: Vec<String>,
    pub no_tags: Vec<String>,
    /// `update --edit` (§9): source the change from $EDITOR instead of the field
    /// flags — mutually exclusive with them (they would race over the payload).
    pub edit: bool,
    /// The per-op `--remote`/`--center` store-remote override — the top tier of
    /// the ONE §12 ladder, accepted by every mutating verb exactly as by
    /// `prime`/`sync` (bl-c2de). Ephemeral: it shapes this invocation's binding
    /// and persists nothing (durability is `origin` or the XDG `task-remote`).
    pub remote: Option<String>,
    pub positionals: Vec<String>,
}

/// The value-taking short flags — the ones the glued form ([`unglue`]) applies
/// to. One home for "which shorts take a value"; the match in [`parse`] names
/// each alongside its long twin.
const SHORT_VALUED: [&str; 3] = ["-m", "-p", "-t"];

/// Expand a glued short flag (`-p1`) to its split form (`-p 1`) — the getopt
/// convention every git/ls-shaped CLI honors. Only the [`SHORT_VALUED`] shorts
/// glue: `--long=value` is not a form this parser speaks, and an unknown `-x1`
/// still falls through to [`parse`]'s unexpected-flag error. Expansion stops at
/// the `--` end-of-options separator — beyond it nothing is a flag, so nothing
/// glues.
fn unglue(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut rest = args.iter();
    for arg in rest.by_ref() {
        if arg == "--" {
            out.push(arg.clone());
            break;
        }
        match arg.split_at_checked(2) {
            Some((flag, glued)) if SHORT_VALUED.contains(&flag) && !glued.is_empty() => {
                out.push(flag.to_string());
                out.push(glued.to_string());
            }
            _ => out.push(arg.clone()),
        }
    }
    out.extend(rest.cloned());
    out
}

/// Parse argv into [`Flags`]. A leading-`-` token that is not a known flag is an
/// error; everything else is a positional. `--as` defaults to `default_actor`.
/// Glued short flags (`-p1`) are accepted as their split form (`-p 1`); a `--`
/// ends option parsing (getopt), so every later token is a positional however
/// `-`-leading — the seam for shelling an untrusted title (`bl create -- "$T"`).
pub(super) fn parse(args: &[String], default_actor: &str) -> io::Result<Flags> {
    let args = &unglue(args);
    let mut f = Flags { actor: default_actor.to_string(), ..Flags::default() };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                f.positionals.extend(args[i + 1..].iter().cloned());
                break;
            }
            "--as" => f.actor = value(args, &mut i, "--as")?,
            "-m" | "--message" => f.message = Some(value(args, &mut i, "-m")?),
            "--body" => f.body = Some(value(args, &mut i, "--body")?),
            "--title" => f.title = Some(value(args, &mut i, "--title")?),
            "--parent" => f.parent = Some(value(args, &mut i, "--parent")?),
            "--no-parent" => f.no_parent = true,
            "--subtask-of" => f.subtask_of = Some(value(args, &mut i, "--subtask-of")?),
            "--blocks" => f.blocks.push(value(args, &mut i, "--blocks")?),
            "--needs" => f.needs.push(value(args, &mut i, "--needs")?),
            "--no-needs" => f.no_needs.push(value(args, &mut i, "--no-needs")?),
            "-p" | "--priority" => {
                let v = value(args, &mut i, "-p")?;
                f.priority = Some(v.parse().map_err(|_| other(format!("-p: '{v}' is not an integer")))?);
            }
            "--no-priority" => f.no_priority = true,
            "-t" | "--tag" => f.tags.push(value(args, &mut i, "-t")?),
            "--no-tag" => f.no_tags.push(value(args, &mut i, "--no-tag")?),
            "--edit" => f.edit = true,
            // The store-remote ladder, shared verbatim with prime/sync/import.
            flag @ ("--remote" | "--center") => {
                crate::checkout::apply_remote(&mut f.remote, flag, value(args, &mut i, flag)?);
            }
            flag if flag.starts_with('-') => return Err(other(format!("unexpected flag '{flag}'"))),
            _ => f.positionals.push(args[i].clone()),
        }
        i += 1;
    }
    Ok(f)
}

/// The value following a `--flag`, advancing the cursor; a missing value errors.
fn value(args: &[String], i: &mut usize, flag: &str) -> io::Result<String> {
    *i += 1;
    args.get(*i).cloned().ok_or_else(|| other(format!("{flag} needs a value")))
}
