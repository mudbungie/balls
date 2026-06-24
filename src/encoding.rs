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
