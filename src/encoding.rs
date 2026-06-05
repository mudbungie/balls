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
mod tests {
    use super::*;

    #[test]
    fn unreserved_bytes_pass_through_untouched() {
        assert_eq!(percent_encode("aZ09-._~"), "aZ09-._~");
    }

    #[test]
    fn a_path_encodes_to_one_slash_free_component() {
        let enc = percent_encode("/home/mark/dev/balls");
        assert_eq!(enc, "%2Fhome%2Fmark%2Fdev%2Fballs");
        assert!(!enc.contains('/'));
    }

    #[test]
    fn dot_dot_is_neutralized_only_in_slashes() {
        // `.` is unreserved, so `..` survives, but a separator never does —
        // the encoded value can never escape its parent directory.
        assert_eq!(percent_encode("../x"), "..%2Fx");
    }

    #[test]
    fn a_remote_url_encodes_to_one_component() {
        assert_eq!(
            percent_encode("git@github.com:mudbungie/balls.git"),
            "git%40github.com%3Amudbungie%2Fballs.git"
        );
    }

    #[test]
    fn both_hex_nibbles_appear_uppercase() {
        // 0x1f exercises the low nibble's `< 10` and high nibble's `>= 10`
        // arms in one byte; 0xf0-style bytes the reverse.
        assert_eq!(percent_encode("\x1f"), "%1F");
        assert_eq!(percent_encode("\u{007f}"), "%7F");
    }

    #[test]
    fn multibyte_utf8_encodes_every_continuation_byte() {
        assert_eq!(percent_encode("é"), "%C3%A9");
    }
}
