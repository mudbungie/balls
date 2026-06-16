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
