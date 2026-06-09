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
mod tests {
    use super::*;

    #[test]
    fn the_default_scheme_is_todays_scheme() {
        let scheme = IdScheme::default();
        assert_eq!(scheme.prefix, "bl-");
        assert_eq!(scheme.length, 4);
        assert_eq!(scheme.alphabet, "0123456789abcdef");
    }

    #[test]
    fn generate_with_maps_bytes_through_the_alphabet() {
        // 0->'0', 17%16=1->'1', 32%16=0->'0', 48%16=0->'0'.
        let bytes = [0u8, 17, 32, 48];
        let mut i = 0;
        let id = IdScheme::default().generate_with(&mut || {
            let b = bytes[i];
            i += 1;
            b
        });
        assert_eq!(id, "bl-0100");
    }

    #[test]
    fn generate_with_rejects_biased_bytes_and_redraws() {
        // alphabet of 10 → ceiling 250; byte 255 must be rejected, 7 accepted.
        let scheme = IdScheme {
            prefix: "x".to_string(),
            length: 1,
            alphabet: "0123456789".to_string(),
        };
        let bytes = [255u8, 7];
        let mut i = 0;
        let id = scheme.generate_with(&mut || {
            let b = bytes[i];
            i += 1;
            b
        });
        assert_eq!(id, "x7");
    }

    #[test]
    fn generate_draws_a_valid_id_from_entropy() {
        let scheme = IdScheme::default();
        let id = scheme.generate();
        assert!(id.starts_with("bl-"));
        assert_eq!(id.len(), "bl-".len() + 4);
        assert!(is_valid(&id));
        assert!(id["bl-".len()..]
            .chars()
            .all(|c| scheme.alphabet.contains(c)));
    }

    #[test]
    fn validation_is_string_safety() {
        assert!(is_valid("bl-1a2f"));
        assert!(is_valid("A"));
        assert!(is_valid("custom_id-9"));
        assert!(!is_valid("")); // empty
        assert!(!is_valid("-bl-1")); // leading hyphen
        assert!(!is_valid("bl/1")); // path separator
        assert!(!is_valid("bl.1")); // dot
        assert!(!is_valid("bl 1")); // whitespace
    }
}
