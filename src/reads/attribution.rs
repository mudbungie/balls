//! The §9 comment byline — who wrote which comment, DERIVED from `git blame`
//! and stored nowhere (bl-236c). Sibling of [`super::journal`] and the same
//! kind of thing: a pure read-side projection over store history, folded into
//! human `bl show` alone. Bedrock `--json` carries stored state, so it never
//! carries a byline and never pays the blame.
//!
//! `bl comment` (§9, bl-d136) appends to the body under a rule and stamps
//! NOTHING — the commit already records who and when, and a second copy in the
//! body would drift. That leaves co-location as the only gap: six comments in
//! one body and no way to tell whose is whose without `git log -p`. This closes
//! it by asking git at render time.
//!
//! **The commit boundary IS the comment boundary.** An append is one commit, so
//! its lines are its lines: group the body's lines by their blame commit, keep
//! the groups whose §5 `bl-op` trailer is `comment`, and hang ONE added render
//! line off each. That is why no marker parsing is needed anywhere here.
//!
//! **The `---` rule is never read** (bl-d136 states it; this does not weaken
//! it). Nothing here searches for a rule, counts rules, splits on one, or
//! suppresses one. No body byte is inspected at all: the body's LINE COUNT is
//! the only thing read off it, to align git's per-line answer with the tail of
//! the file, and every byte passes through to the render unaltered.
//!
//! The byline hangs at the END of its comment's lines, not the start. That is
//! forced by never reading the rule: the append is `\n\n---\n\n{text}\n`, so a
//! comment commit's FIRST lines are always the blank/rule/blank decoration —
//! a byline there sits above the rule and directly under the PREVIOUS comment's
//! text, reading as that one's. The commit's LAST line is always the comment's
//! own last line of text, so a byline there is unambiguous without balls ever
//! knowing a rule exists. (The ball specified "head"; implementation found the
//! misattribution — see `docs/design/bl-236c-comment-attribution.md`.)
//!
//! Degradation is honest and never an error: whatever blame says is what
//! renders. An imported ball collapses onto the import commit (op `import`, not
//! `comment`) and renders bare — that IS who wrote that file. A squashed or
//! rewritten store collapses the same way. A ball whose file git cannot blame
//! at all (never committed, or a store with no history) renders bare too:
//! blame is the one input, and nothing said means nothing rendered.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use super::Style;
use crate::git;

/// The `\x1f` field separator the byline `--format` uses — a control byte no
/// §5 field carries, so the machine fields split unambiguously.
const FIELD: char = '\u{1f}';

/// The `\x1e` record separator opening each commit's record: a `valueonly`
/// trailer expansion ends in git's own newline, so a record spans lines and
/// only a control byte can delimit one.
const RECORD: char = '\u{1e}';

/// `body` with a byline hung under each of its `comment`-op regions — the human
/// `bl show` projection of a ball's markdown (bl-236c). `rev` is the revision
/// the rendered content comes from (`HEAD` for a live ball, the deletion's
/// parent for a dead one, [`super::history::Dead::rev`]).
///
/// An EMPTY body makes no blame call at all — there is nothing to attribute.
/// Otherwise it costs one `git blame` plus one `git log` over the blamed set,
/// the same cost shape as the journal walk and paid only by the human render.
pub(crate) fn annotate(store: &Path, rev: &str, id: &str, body: &str, style: &Style) -> io::Result<String> {
    if body.is_empty() {
        return Ok(String::new());
    }
    // Blame is the ONE input. When git cannot answer for this path — a ball
    // whose file was never committed, a store with no history — there is no
    // attribution to render and the body passes through bare. Same rule as
    // every other case ("render what blame says"), with nothing said.
    let Ok(shas) = blamed(store, rev, &format!("tasks/{id}.md")) else {
        return Ok(body.to_string());
    };
    let bylines = bylines(store, &shas, style)?;
    Ok(rendered(body, &shas, &bylines))
}

/// One commit sha per line of the file at `rev`, in file order — git's
/// `blame --porcelain` line map, asked for as structured output rather than
/// hand-parsed (the §5 no-hand-rolled-parser discipline on the read side).
fn blamed(store: &Path, rev: &str, path: &str) -> io::Result<Vec<String>> {
    let out = git::run(store, &["blame", "--porcelain", rev, "--", path], None)?;
    Ok(out.lines().filter_map(header_sha).map(str::to_string).collect())
}

/// The sha a `--porcelain` header line opens, or `None` for git's per-commit
/// metadata lines and the file's own content lines. A metadata line's first
/// token is its key (`author`, `summary`, `previous`, …), never a sha; a
/// content line is TAB-prefixed, and a tab is not a hex digit — so the one
/// test "first token is 40 hex bytes" tells all three apart whatever the file
/// holds, without the renderer looking at the content.
fn header_sha(line: &str) -> Option<&str> {
    let sha = line.split(' ').next().expect("split always yields a first field");
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then_some(sha)
}

/// The rendered byline for each DISTINCT blamed commit whose §5 `bl-op` trailer
/// reads `comment` — ONE `git log --no-walk` over the blamed set, no history
/// walk. A commit under any other op is simply absent from the map, which is
/// how a `create` body, a `--body` rewrite, an `--edit` and an import all render
/// bare with no case of their own.
///
/// The actor is the `bl-actor` trailer `--as` set, with no author fallback:
/// only a balls `comment` commit reaches the map, and one always carries it.
fn bylines(store: &Path, shas: &[String], style: &Style) -> io::Result<HashMap<String, String>> {
    let mut revs: Vec<&str> = shas.iter().map(String::as_str).collect();
    revs.sort_unstable();
    revs.dedup();
    let fmt = format!(
        "--format={RECORD}%H{FIELD}%ct{FIELD}%(trailers:key=bl-op,valueonly=true)\
         {FIELD}%(trailers:key=bl-actor,valueonly=true)"
    );
    let mut args = vec!["log", "--no-walk", &fmt];
    args.extend(revs);
    args.push("--");
    let log = git::run(store, &args, None)?;
    let mut bylines = HashMap::new();
    for record in log.split(RECORD).skip(1) {
        let mut f = record.splitn(4, FIELD);
        let sha = f.next().expect("splitn always yields a first field");
        let ct = f.next().unwrap_or_default().trim();
        let op = f.next().unwrap_or_default().trim();
        let actor = f.next().unwrap_or_default().trim();
        if op == "comment" {
            let at: i64 = ct.parse().expect("git %ct is an integer unix timestamp");
            bylines.insert(sha.to_string(), style.byline(at, actor));
        }
    }
    Ok(bylines)
}

/// `body` with each blamed group's byline emitted after the group's last line.
///
/// The body is the TAIL of `tasks/<id>.md` (frontmatter, fence, then body), so
/// its lines are the last `n` of git's per-line answer — no frontmatter parsing
/// and no offset arithmetic over the fence. Body bytes are copied verbatim,
/// newline-for-newline: the byline is an ADDED line, never a rewrite.
fn rendered(body: &str, shas: &[String], bylines: &HashMap<String, String>) -> String {
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let at = &shas[shas.len().saturating_sub(lines.len())..];
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(line);
        // The group ends where the next line's commit differs (and at the last
        // line, where there is no next commit at all).
        if at.get(i) == at.get(i + 1) {
            continue;
        }
        let Some(byline) = at.get(i).and_then(|sha| bylines.get(sha)) else {
            continue;
        };
        // A body need not end in a newline (`--body` can leave it bare), and a
        // byline is its own line whatever the text above it did.
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(byline);
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "attribution_tests.rs"]
mod tests;
