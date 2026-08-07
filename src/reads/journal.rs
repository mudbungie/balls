//! The §9 journal render — the store-branch history of one ball's
//! `tasks/<id>.md`, folded into human `bl show` (bl-0e16). Notes ride the §5
//! commit's free body (`-m`, §9 note-append); there is no journal field, so the
//! journal IS git history and this module is a pure read-side projection: one
//! `git log` walk per show, oldest-first, one entry per commit — timestamp, op,
//! actor, and the note. Derived means human-only: bedrock `--json` never carries
//! it (§3).
//!
//! That is exactly why `bl comment` (§9, bl-d136) appends to the BODY instead:
//! a note here is invisible to `--json`, so a note that must reach both
//! projections has to land in stored state. The two are different facts with
//! different reach, not two spellings of one — history for what happened, the
//! body for what the ball says.
//!
//! The §5 no-hand-rolled-parser discipline holds on the read side too: the
//! walk asks git for the trailer VALUES (`%(trailers:key=…)`) and for the
//! block itself (`%(trailers)`), and the note is `%b` minus git's own answer
//! for the block — balls never decides where a trailer paragraph starts.

use std::fmt::Write;
use std::io;
use std::path::Path;

use crate::civil::iso8601;
use crate::git;

/// The `\x1f` field separator the walk's `--format` uses — a control byte no
/// §5 field carries; the free-text body rides LAST so a stray byte in a note
/// cannot shift the machine fields.
const FIELD: char = '\u{1f}';

/// The `\x1e` record separator opening each commit's record, so the split
/// yields one whole record per commit however many lines the body spans.
const RECORD: char = '\u{1e}';

/// The rendered `journal` section for `tasks/<id>.md` — oldest-first, one
/// entry per store commit that touched the file; `""` when the path has no
/// history yet. Live and dead ids walk identically: a closed ball's file is
/// older content (§2), and its deletion commit is the journal's last entry.
pub(crate) fn section(store: &Path, id: &str) -> io::Result<String> {
    let path = format!("tasks/{id}.md");
    let fmt = format!(
        "--format={RECORD}%ct{FIELD}%an{FIELD}%(trailers:key=bl-op,valueonly=true){FIELD}\
         %(trailers:key=bl-actor,valueonly=true){FIELD}%(trailers){FIELD}%b"
    );
    let log = git::run(store, &["log", "--reverse", &fmt, "--", &path], None)?;
    let mut out = String::new();
    for record in log.split(RECORD).skip(1) {
        entry(&mut out, record);
    }
    if !out.is_empty() {
        out.insert_str(0, "  journal\n");
    }
    Ok(out)
}

/// Render one commit's entry: a `<date>  <op>  <actor>` line, then the note
/// (the §5 free body) indented under it. The actor is the `bl-actor` trailer;
/// a non-balls commit (no trailer block) falls back to the git author, and
/// its whole body reads as the note — history stays total, never filtered.
fn entry(out: &mut String, record: &str) {
    let mut f = record.splitn(6, FIELD);
    let ct = f.next().expect("splitn always yields a first field");
    let ct: i64 = ct.parse().expect("git %ct is an integer unix timestamp");
    let author = f.next().unwrap_or_default();
    let op = first_line(f.next().unwrap_or_default());
    let actor = first_line(f.next().unwrap_or_default());
    let trailers = f.next().unwrap_or_default();
    let body = f.next().unwrap_or_default();
    let actor = if actor.is_empty() { author.trim() } else { actor };
    let _ = writeln!(out, "    {}  {:<9}{}", iso8601(ct), op, actor);
    // `%b` carries git's entry-separator newline the block itself lacks, so
    // both sides trim before the subtraction.
    let note = body.trim_end();
    let note = note.strip_suffix(trailers.trim_end()).unwrap_or(note).trim();
    for line in note.lines() {
        // A note's own blank line stays bare — no trailing indent whitespace.
        let _ = if line.is_empty() { writeln!(out) } else { writeln!(out, "      {line}") };
    }
}

/// One value from a `valueonly` trailer expansion — the first line (git prints
/// one per line when a key repeats), `""` when the trailer is absent.
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or_default().trim()
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
