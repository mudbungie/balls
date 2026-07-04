//! §1 percent-encoding — the one naming primitive.
//!
//! Identity in the layout comes from natural names used directly, never from a
//! hash (§1: "URLs and paths are percent-encoded, never hashed, so directory
//! names stay inspectable"). [`percent_encode`] turns any string — an absolute
//! invocation path, a remote URL — into a single inspectable path component:
//! RFC 3986 unreserved bytes pass through, every other byte becomes `%XX`, and
//! the result contains no `/`, so a slash-bearing input occupies exactly one
//! directory level and a `..` in a foreign string is neutralized.
//!
//! Pure, std-only, byte-level: this runs on every `bl` invocation, so it is a
//! single pass over the bytes, not a regex.

/// Percent-encode `s` into one path component per RFC 3986.
///
/// Unreserved characters (`A-Z a-z 0-9 - . _ ~`) pass through; every other
/// byte — including UTF-8 continuation bytes and `/` — becomes `%XX` with
/// uppercase hex. The output is therefore a single slash-free component.
#[must_use]
pub fn percent_encode(s: &str) -> String {
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

/// Decode a [`percent_encode`]d component back to its original string, or `None`
/// when a `%` is not followed by two uppercase-hex digits (a hand-made or
/// foreign directory name) or the bytes are not UTF-8. The exact inverse of the
/// encoder, so a `clones/<pct-enc-path>/` directory name recovers the enrolled
/// checkout's path (bl-5965 fleet labels); an entry the encoder never wrote just
/// declines to decode and is skipped.
#[must_use]
pub fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            out.push(hex_val(bytes.next()?)? << 4 | hex_val(bytes.next()?)?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

/// One uppercase-hex digit (`0-9A-F`) as its 0..=15 value — the [`hex_nibble`]
/// inverse. `None` on any other byte, so a malformed `%XX` escape declines.
const fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Map a 4-bit value (0..=15) to its uppercase hex byte. Callers always pass a
/// nibble (`>> 4` or `& 0x0f` of a byte), so an exhaustive `if` with no dead
/// arm keeps line coverage whole.
const fn hex_nibble(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'A' + n - 10
    }
}

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
