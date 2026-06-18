//! Per-command help DATA — the usage line, flag list, and 1-2 examples for each
//! [`Verb`], the depth `bl <cmd> --help` / `bl help <cmd>` surface (and the
//! footer [`crate::run`] prints when a command is mis-invoked). The companion to
//! [`super::directory`]: the directory is the "which command", this is the
//! "which flags". Kept a sibling of [`super`] so `help.rs` stays the renderer and
//! this stays the text (the bl-19c4/decomposition convention).
//!
//! The `match` is exhaustive over [`Verb`] (no `_` arm), so a new verb cannot be
//! added without authoring its help — the same single-source discipline that
//! keeps [`Verb::summary`] honest, one rung deeper.

use crate::verb::Verb;

/// One command's discoverable surface: its `usage:` line, the flags it accepts
/// (token, gloss), and a worked example or two. All `'static` — help is text,
/// not state.
pub(crate) struct CommandHelp {
    pub(crate) usage: &'static str,
    pub(crate) flags: &'static [(&'static str, &'static str)],
    pub(crate) examples: &'static [&'static str],
}

/// The help for one verb (see [`CommandHelp`]). Exhaustive over [`Verb`].
#[allow(clippy::too_many_lines)] // a flat data table, one arm per verb — the length is the data, not branching
pub(crate) fn command_help(verb: Verb) -> CommandHelp {
    match verb {
        Verb::Create => CommandHelp {
            usage: "bl create \"TITLE\" [--body B] [-p N] [-t TAG] [--parent ID] [--subtask-of ID] [--needs ID[:OP]] [--blocks OP|ID:OP] [-m MSG] [--as ID] [-- TITLE]",
            flags: &[
                ("--body B", "set the task's markdown body (the living document)"),
                ("-p, --priority N", "priority — higher sorts first in `bl list`"),
                ("-t, --tag TAG", "add a tag (repeatable; the flag is --tag, not --tags)"),
                ("--parent ID", "containment only — builds the display tree, gates nothing"),
                ("--subtask-of ID", "child of ID and gate ITS claim (the everyday subtask spelling)"),
                ("--needs ID[:OP]", "add a blocker on this task (default OP=claim)"),
                ("--blocks OP|ID:OP", "gate another task's op on this one (create-only)"),
                ("-m MSG", "commit note for the store journal"),
                ("--as ID", "worker identity"),
                ("--", "end option parsing — shell an untrusted, `-`-leading title"),
            ],
            examples: &[
                "bl create \"Fix the parser\" --body \"repro: bl create -x\"",
                "bl create \"wire the auth endpoint\" --subtask-of bl-1a2b",
            ],
        },
        Verb::Claim => CommandHelp {
            usage: "bl claim <id> [--as ID] [--remote URL]",
            flags: &[("--as ID", "worker identity"), ("--remote URL", "per-op store remote (the §12 ladder's top tier)")],
            examples: &["bl claim bl-1a2b --as alice"],
        },
        Verb::Unclaim => CommandHelp {
            usage: "bl unclaim <id> [--as ID] [--remote URL]",
            flags: &[("--as ID", "worker identity"), ("--remote URL", "per-op store remote")],
            examples: &["bl unclaim bl-1a2b"],
        },
        Verb::Update => CommandHelp {
            usage: "bl update <id> [--edit] [--title T] [--body B] [--parent ID|--no-parent] [-p N|--no-priority] [-t TAG|--no-tag TAG] [--needs ID[:OP]|--no-needs ID] [key=value] [-m MSG]",
            flags: &[
                ("--edit", "open the task in $EDITOR (human-only; excludes the field flags)"),
                ("--title T", "retitle"),
                ("--body B", "rewrite the markdown body"),
                ("--parent ID / --no-parent", "set or clear the parent pointer"),
                ("-p N / --no-priority", "set or clear priority"),
                ("-t TAG / --no-tag TAG", "add or drop a tag"),
                ("--needs ID[:OP] / --no-needs ID", "add or unlink one of this task's blockers"),
                ("key=value", "set a preserved extra field (a bare key= removes it)"),
                ("-m MSG", "commit note — a zero-edit update appends a progress note"),
                ("--as ID", "worker identity"),
            ],
            examples: &[
                "bl update bl-1a2b --body \"now waiting on the upstream release\"",
                "bl update bl-1a2b -m \"progress note (rides git history, not the body)\"",
            ],
        },
        Verb::Close => CommandHelp {
            usage: "bl close <id> [-m MSG] [--as ID] [--remote URL]",
            flags: &[("-m MSG", "commit note"), ("--as ID", "worker identity"), ("--remote URL", "per-op store remote")],
            examples: &["bl close bl-1a2b -m \"shipped\""],
        },
        Verb::Import => CommandHelp {
            usage: "bl import [--legacy[=REF]] [-m MSG] [--as ID]   # bulk-create from JSON on stdin; won't overwrite existing ids",
            flags: &[
                ("--legacy[=REF]", "migrate a pre-greenfield store instead (preview: `bl list --legacy`)"),
                ("-m MSG", "commit note"),
                ("--as ID", "worker identity"),
            ],
            examples: &[
                "cat new-tasks.json | bl import        # create new tasks; use `bl update` to modify existing ones",
                "bl import --legacy                    # migrate an old store",
            ],
        },
        Verb::Show => CommandHelp {
            usage: "bl show <id> [--json] [--plain] [--legacy[=REF]]",
            flags: &[
                ("--json", "lossless machine record (the bedrock; feeds `bl import`)"),
                ("--plain", "no color or status glyphs"),
                ("--legacy[=REF]", "project one ball from a legacy store"),
            ],
            examples: &["bl show bl-1a2b", "bl show bl-1a2b --json"],
        },
        Verb::List => CommandHelp {
            usage: "bl list [-s ready|blocked|claimed|closed] [--all] [--tag T] [--since YYYY-MM-DD] [--until YYYY-MM-DD] [--json] [--plain] [--legacy]",
            flags: &[
                ("-s, --status RUNG", "filter to one status (ready|blocked|claimed|closed)"),
                ("--all", "include closed tasks (live + dead)"),
                ("--tag T", "filter by tag (repeatable, AND)"),
                ("--since YYYY-MM-DD", "tasks updated on or after the date"),
                ("--until YYYY-MM-DD", "tasks updated on or before the date"),
                ("--json", "lossless machine records"),
                ("--plain", "no color or status glyphs"),
                ("--legacy[=REF]", "preview a legacy store's live set"),
            ],
            examples: &["bl list -s ready", "bl list -s claimed"],
        },
        Verb::Prime => CommandHelp {
            usage: "bl prime [--as ID] [--remote URL] [--center URL] [--install CENTER] [--stealth]",
            flags: &[
                ("--as ID", "worker identity"),
                ("--remote URL", "per-op store remote (the §12 ladder's top tier)"),
                ("--center URL", "alias of --remote (the federation framing)"),
                ("--install CENTER", "adopt config/ from CENTER"),
                ("--stealth", "opt out of any store remote, durably — the store stays local"),
            ],
            examples: &["bl prime --as alice", "bl prime --remote git@host:repo.git"],
        },
        Verb::Sync => CommandHelp {
            usage: "bl sync [BRANCH] [--as ID] [--remote URL] [--center URL]",
            flags: &[
                ("--as ID", "worker identity"),
                ("--remote URL", "per-op store remote"),
                ("--center URL", "alias of --remote"),
            ],
            examples: &["bl sync"],
        },
        Verb::Install => CommandHelp {
            usage: "bl install [PATH] [--from REF] [--to REF] [--bin NAME=PATH] [--as ID]",
            flags: &[
                ("PATH", "committed path to copy (default: config)"),
                ("--from REF", "source branch (default: the configured upstream)"),
                ("--to REF", "target branch (default: the landing balls/config)"),
                ("--bin NAME=PATH", "bind a plugin binary (a landing-targeted install only)"),
                ("--as ID", "worker identity"),
            ],
            examples: &["bl install config"],
        },
        Verb::Conf => CommandHelp {
            usage: "bl conf [<key>]   |   bl conf set|append|prepend|remove <key> <value...>",
            flags: &[
                ("(no args)", "dump every resolved value + its source layer + the file paths"),
                ("<key>", "print one value with its provenance"),
                ("set <key> <value>", "write a value — the key implies its config file"),
                ("append|prepend|remove <key> <name>", "compose one name in a [hooks] list"),
            ],
            examples: &[
                "bl conf",
                "bl conf set task-remote git@host:repo.git",
                "bl conf append list myplugin",
            ],
        },
    }
}
