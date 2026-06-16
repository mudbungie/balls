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
