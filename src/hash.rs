//! Minimal in-tree SHA-1 + lowercase-hex encoder. Vendored in place of
//! the `sha1` and `hex` crates so the dependency tree does not carry
//! the full RustCrypto stack (~8 transitive crates) for the two call
//! sites in this codebase. Output is byte-identical to
//! `hex::encode(sha1::Sha1::digest(...))`, which existing on-disk
//! artifacts (task ids, stealth-store directories) depend on — see
//! `task::Task::generate_id` and `store_paths::stealth_tasks_dir` for
//! the load-bearing-hash-value rationale (footprint audit bl-32f8,
//! decision bl-bd85, vendor lever bl-cb4e).

use std::fmt::Write as _;

/// Compute the SHA-1 of `bytes` (FIPS 180-4) and return it as a
/// 40-character lowercase hex string.
///
/// # Panics
/// Never in practice: every `.expect()` site is infallible by
/// construction (slice lengths come from `chunks_exact` or fixed-size
/// sub-slices of an owned buffer; writing to a `String` is infallible;
/// `usize` fits in `u64` on every supported target).
#[allow(clippy::many_single_char_names)]
pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut h: [u32; 5] = [
        0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476, 0xc3d2_e1f0,
    ];
    let bit_len = u64::try_from(bytes.len())
        .expect("usize fits in u64")
        .wrapping_mul(8);

    let mut chunks = bytes.chunks_exact(64);
    for chunk in &mut chunks {
        let block: &[u8; 64] = chunk.try_into().expect("chunks_exact yields 64");
        compress(&mut h, block);
    }
    let tail = chunks.remainder();

    // Append 0x80, then zeros, then the big-endian bit length in the
    // last 8 bytes of one or two final blocks (two if the bit-length
    // tail won't fit alongside the original message tail and the 0x80).
    let mut pad = [0u8; 128];
    pad[..tail.len()].copy_from_slice(tail);
    pad[tail.len()] = 0x80;
    let pad_blocks: usize = if tail.len() >= 56 { 2 } else { 1 };
    let end = pad_blocks * 64;
    pad[end - 8..end].copy_from_slice(&bit_len.to_be_bytes());

    let first: &[u8; 64] = pad[..64].try_into().expect("64-byte slice");
    compress(&mut h, first);
    if pad_blocks == 2 {
        let second: &[u8; 64] = pad[64..128].try_into().expect("64-byte slice");
        compress(&mut h, second);
    }

    let mut out = String::with_capacity(40);
    for word in h {
        write!(out, "{word:08x}").expect("write to String never fails");
    }
    out
}

#[allow(clippy::many_single_char_names, clippy::needless_range_loop)]
fn compress(h: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for (i, word_bytes) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes(word_bytes.try_into().expect("4 bytes"));
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
    for (i, &wi) in w.iter().enumerate() {
        let (f, k) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5a82_7999_u32),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let t = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(wi);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = t;
    }
    h[0] = h[0].wrapping_add(a);
    h[1] = h[1].wrapping_add(b);
    h[2] = h[2].wrapping_add(c);
    h[3] = h[3].wrapping_add(d);
    h[4] = h[4].wrapping_add(e);
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
