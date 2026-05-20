//! FIPS 180-4 SHA-1 known-answer tests, chosen to cover every branch
//! and loop of `sha1_hex`: short single-block padding, two-block
//! padding (message tail ≥ 56), and the multi-block full-input loop.

use super::sha1_hex;

#[test]
fn empty_string_matches_fips_vector() {
    // tail.len() == 0, pad_blocks == 1.
    assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

#[test]
fn abc_matches_fips_vector() {
    // tail.len() == 3 < 56, pad_blocks == 1.
    assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
}

#[test]
fn fifty_six_byte_message_triggers_two_block_padding() {
    // tail.len() == 56 ⇒ pad_blocks == 2. The canonical FIPS 180-4
    // "abcdbcdecdef..." 56-byte vector.
    let m = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    assert_eq!(m.len(), 56);
    assert_eq!(sha1_hex(m), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
}

#[test]
fn one_million_a_exercises_full_block_loop() {
    // 1_000_000 bytes → 15_625 full 64-byte blocks via chunks_exact,
    // remainder 0, then a single padding block. Canonical FIPS vector.
    let m = vec![b'a'; 1_000_000];
    assert_eq!(sha1_hex(&m), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
}
