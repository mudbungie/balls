//! URL canonicalization, percent-encoding, and nested clone paths per
//! [SPEC-clone-layout.md](../docs/SPEC-clone-layout.md) §4.
//!
//! No hashing anywhere (SPEC §2 principle 10): identity comes from
//! natural names used directly. The canonicalization rules here must
//! match the §10 hand-operable shell sequence bit-for-bit, since the
//! SPEC promises a user can recover paths with stock `git`, `jq`, and
//! `jq -Rr @uri`.
//!
//! Three pure functions live here:
//!
//! - [`canonicalize_origin`] — strips scheme/userinfo, drops trailing
//!   `.git` and `/`, lowercases. Matches the §10 `sed` pipeline.
//! - [`percent_encode_component`] — RFC 3986 unreserved + percent-
//!   encoding for everything else, producing a single path component
//!   with no slashes (so a multi-slash branch name does not fan out
//!   into a nondeterministic subtree, and `..` in a foreign string
//!   cannot escape).
//! - [`nested_clone_path`] — strips the leading `/` from the clone's
//!   absolute git-dir path, preserving slashes. `/home/mark/dev/balls`
//!   becomes `home/mark/dev/balls`.
//!
//! The encoded outputs are used as path components in the XDG layout
//! (`trackers/<enc-origin>/<enc-branch>/`, `worktrees/<nested>/`).
//! Frozen golden vectors in tests gate against future drift (SPEC §14.2).

use std::path::{Path, PathBuf};

/// Canonicalize an `origin` URL per SPEC §4 / §10.
///
/// Steps, in order:
/// 1. Strip a leading `[a-z]+://` scheme.
/// 2. Strip a leading `[^@]+@` userinfo segment.
/// 3. Strip a trailing `.git`.
/// 4. Strip a trailing `/`.
/// 5. Lowercase ASCII.
///
/// Pure function on the URL string; no I/O, no validation beyond the
/// transformations above. A URL that fails to match any of the strips
/// passes through that step unchanged — canonicalization never errors.
#[must_use]
pub fn canonicalize_origin(url: &str) -> String {
    let s = strip_scheme(url);
    let s = strip_userinfo(s);
    let s = strip_trailing_dot_git(s);
    let s = strip_trailing_slash(s);
    s.to_ascii_lowercase()
}

fn strip_scheme(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_lowercase() {
        i += 1;
    }
    if i > 0 && bytes.get(i..i + 3) == Some(b"://") {
        &s[i + 3..]
    } else {
        s
    }
}

fn strip_userinfo(s: &str) -> &str {
    match s.find('@') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

fn strip_trailing_dot_git(s: &str) -> &str {
    s.strip_suffix(".git").unwrap_or(s)
}

fn strip_trailing_slash(s: &str) -> &str {
    s.strip_suffix('/').unwrap_or(s)
}

/// Percent-encode one path component per SPEC §4 (RFC 3986 unreserved
/// preserved; everything else `%XX`). The output contains no `/`, so
/// the encoded value occupies exactly one path component — a multi-
/// slash branch name like `balls/tasks` becomes `balls%2Ftasks` and
/// does not fan out into a subtree, and `..` in a foreign string is
/// neutralized.
///
/// Unreserved characters (RFC 3986 §2.3): `A-Z a-z 0-9 - . _ ~`. Every
/// other byte (including multi-byte UTF-8 continuations) is encoded as
/// `%XX` with uppercase hex digits. Matches `jq -Rr @uri`'s output for
/// the URL shapes balls actually sees (HTTPS, SSH-style with `:`,
/// optional `.git`).
///
/// This is hot — `Store::discover` runs it on every `bl` invocation —
/// so the implementation is byte-level, not regex.
#[must_use]
pub fn percent_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_nibble(b >> 4) as char);
            out.push(hex_nibble(b & 0x0f) as char);
        }
    }
    out
}

const fn is_unreserved(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
}

/// Map a 4-bit value (0..=15) to its uppercase hex byte. Callers mask
/// with `& 0x0f` or shift `>> 4` of a u8, so the input is always in
/// range; an exhaustive `if` with no dead arm keeps line coverage at
/// 100% (per the project-wide coverage convention — no dead match
/// branches).
const fn hex_nibble(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'A' + n - 10
    }
}

/// The clone's absolute git-directory path with its leading `/`
/// dropped, slashes preserved. `/home/mark/dev/balls` becomes
/// `home/mark/dev/balls`.
///
/// SPEC §4: "hand-operable: `cd ~/.local/state/balls/worktrees/$(pwd
/// | sed 's|^/||')/`" — the leading-slash strip is the only
/// transformation. Slashes inside the path stay, since the layout
/// nests literally.
///
/// A relative path (no leading `/`) passes through unchanged; on
/// non-Unix targets the leading `/` may be absent. Callers feed
/// `git rev-parse --absolute-git-dir`'s output here, which on Unix
/// always starts with `/` and on Windows uses backslashes — Windows
/// is out of scope for SPEC-clone-layout (the XDG dirs themselves
/// are Linux-specific). The function is a string transform, not a
/// platform abstraction.
#[must_use]
pub fn nested_clone_path(absolute_path: &Path) -> PathBuf {
    let s = absolute_path.to_string_lossy();
    let stripped = s.strip_prefix('/').unwrap_or(&s);
    PathBuf::from(stripped)
}

/// Compile-time constant: the orphan branch name, percent-encoded.
/// SPEC §3 / §4: `<enc-balls-tasks>` is always `balls%2Ftasks`. The
/// bootstrap branch is a binary constant, so its encoded form is too.
pub const ENC_BALLS_TASKS: &str = "balls%2Ftasks";

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
