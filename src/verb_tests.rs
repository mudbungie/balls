use super::*;

#[test]
fn every_verb_round_trips_through_its_token() {
    for v in Verb::ALL {
        assert_eq!(Verb::parse(v.token()), Some(v));
    }
}

#[test]
fn unknown_token_does_not_parse() {
    assert_eq!(Verb::parse("frobnicate"), None);
}

#[test]
fn every_verb_has_a_nonempty_summary() {
    // The `bl help` directory is generated from these, so each must speak.
    for v in Verb::ALL {
        assert!(!v.summary().is_empty(), "{} has no summary", v.token());
    }
}

#[test]
fn a_verb_serializes_as_its_token_and_round_trips() {
    let token = toml::Value::try_from(Verb::Unclaim).unwrap();
    assert_eq!(token.as_str(), Some("unclaim"));
    let back: Verb = toml::Value::String("close".into()).try_into().unwrap();
    assert_eq!(back, Verb::Close);
}

#[test]
fn deserializing_an_unknown_op_is_an_error() {
    let result: Result<Verb, _> = toml::Value::String("frob".into()).try_into();
    assert!(result.unwrap_err().to_string().contains("unknown op 'frob'"));
}

#[test]
fn only_deliverable_verbs_are_mutating() {
    let mutating = [
        Verb::Create,
        Verb::Claim,
        Verb::Unclaim,
        Verb::Update,
        Verb::Close,
        Verb::Import,
    ];
    for v in Verb::ALL {
        let expected = if mutating.contains(&v) {
            OpClass::Mutating
        } else {
            OpClass::Diffless
        };
        assert_eq!(v.class(), expected);
    }
}
