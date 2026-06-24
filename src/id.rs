//! § id generation — the `id_scheme` and the one shipped (random) generator.
//!
//! `id_scheme = { prefix, length, alphabet }`, no generator enum: base balls
//! ships ONE generator (random); non-default generation (timestamp/sequential/
//! uuid) is a `create/pre` plugin via the same id-reassign seam. An id is the
//! filename basename of `tasks/<id>.md` (Model A — id IS the path, never a
//! field), so the only constraint core puts on it is **string-safety**, not a
//! fixed charset: it must be a safe path token on any filesystem.

/// How `create` mints a fresh id: a `prefix` followed by `length` characters
/// drawn from `alphabet`. FIXED, not config — the default (`bl-` + four lower
/// hex digits) is the one scheme base balls ships; a team wanting any other
/// (different prefix/length/alphabet, or a non-random strategy) supplies a
/// `create/pre` plugin via the id-reassign seam. Lowercase sidesteps
/// case-insensitive-FS collisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdScheme {
    pub prefix: String,
    pub length: usize,
    pub alphabet: String,
}

impl Default for IdScheme {
    fn default() -> IdScheme {
        IdScheme {
            prefix: "bl-".to_string(),
            length: 4,
            alphabet: "0123456789abcdef".to_string(),
        }
    }
}

impl IdScheme {
    /// Mint an id, drawing raw bytes from `next`. Each position uses rejection
    /// sampling so every alphabet character is equally likely regardless of
    /// the alphabet's length (no modulo bias). The byte source is injected so
    /// the mapping is testable without entropy; [`IdScheme::generate`] wires in
    /// the system source. Precondition: `alphabet` is non-empty.
    pub fn generate_with(&self, next: &mut dyn FnMut() -> u8) -> String {
        let alphabet = self.alphabet.as_bytes();
        let n = alphabet.len();
        // Largest multiple of `n` that fits in a byte; bytes at/above it would
        // skew the distribution, so we redraw.
        let ceiling = (256 / n) * n;
        let mut id = String::with_capacity(self.prefix.len() + self.length);
        id.push_str(&self.prefix);
        for _ in 0..self.length {
            let mut byte = usize::from(next());
            while byte >= ceiling {
                byte = usize::from(next());
            }
            id.push(alphabet[byte % n] as char);
        }
        id
    }

    /// Mint an id from system entropy — the one generator base balls ships.
    ///
    /// # Panics
    /// Only if the system entropy source is unavailable, which does not occur
    /// on a supported platform.
    pub fn generate(&self) -> String {
        self.generate_with(&mut || {
            let mut byte = [0u8; 1];
            getrandom::fill(&mut byte).expect("system entropy unavailable");
            byte[0]
        })
    }
}

/// Whether `id` is a safe path token: `^[A-Za-z0-9][A-Za-z0-9_-]*$`. No `/`,
/// `.`, whitespace or shell metacharacters, and no leading `-`. This is the
/// only check core makes — it bounds safety, not aesthetics, so a plugin may
/// assign any id that survives it. Applies to the full id (prefix included).
pub fn is_valid(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
#[path = "id_tests.rs"]
mod tests;
