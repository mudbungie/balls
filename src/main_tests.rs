use super::*;

#[test]
fn bare_hex_gets_prefixed() {
    assert_eq!(normalize_id("534c".into()), "bl-534c");
    assert_eq!(normalize_id("abcdef0123".into()), "bl-abcdef0123");
}

#[test]
fn already_prefixed_is_unchanged() {
    assert_eq!(normalize_id("bl-534c".into()), "bl-534c");
}

#[test]
fn non_hex_is_unchanged() {
    assert_eq!(normalize_id("not-an-id".into()), "not-an-id");
    assert_eq!(normalize_id("xyz".into()), "xyz");
}

#[test]
fn empty_is_unchanged() {
    assert_eq!(normalize_id(String::new()), "");
}

#[test]
fn opt_and_vec_helpers_normalize_each() {
    assert_eq!(normalize_opt(Some("534c".into())), Some("bl-534c".into()));
    assert_eq!(normalize_opt(None), None);
    assert_eq!(
        normalize_vec(vec!["534c".into(), "bl-1e60".into()]),
        vec!["bl-534c".to_string(), "bl-1e60".to_string()]
    );
}

#[test]
fn normalize_dep_add() {
    let DepCmd::Add { task, depends_on } = normalize_dep(DepCmd::Add {
        task: "534c".into(),
        depends_on: "1e60".into(),
    }) else {
        panic!("wrong variant");
    };
    assert_eq!(task, "bl-534c");
    assert_eq!(depends_on, "bl-1e60");
}

#[test]
fn normalize_dep_rm() {
    let DepCmd::Rm { task, depends_on } = normalize_dep(DepCmd::Rm {
        task: "534c".into(),
        depends_on: "1e60".into(),
    }) else {
        panic!("wrong variant");
    };
    assert_eq!(task, "bl-534c");
    assert_eq!(depends_on, "bl-1e60");
}

#[test]
fn normalize_dep_tree_some_and_none() {
    let DepCmd::Tree { id, json } = normalize_dep(DepCmd::Tree {
        id: Some("534c".into()),
        json: true,
    }) else {
        panic!("wrong variant");
    };
    assert_eq!(id, Some("bl-534c".into()));
    assert!(json);

    let DepCmd::Tree { id, json } = normalize_dep(DepCmd::Tree {
        id: None,
        json: false,
    }) else {
        panic!("wrong variant");
    };
    assert_eq!(id, None);
    assert!(!json);
}

#[test]
fn normalize_link_add() {
    let LinkCmd::Add {
        task,
        link_type,
        target,
    } = normalize_link(LinkCmd::Add {
        task: "534c".into(),
        link_type: "relates_to".into(),
        target: "1e60".into(),
    })
    else {
        panic!("wrong variant");
    };
    assert_eq!(task, "bl-534c");
    assert_eq!(link_type, "relates_to");
    assert_eq!(target, "bl-1e60");
}

#[test]
fn normalize_link_rm() {
    let LinkCmd::Rm {
        task,
        link_type,
        target,
    } = normalize_link(LinkCmd::Rm {
        task: "534c".into(),
        link_type: "relates_to".into(),
        target: "1e60".into(),
    })
    else {
        panic!("wrong variant");
    };
    assert_eq!(task, "bl-534c");
    assert_eq!(link_type, "relates_to");
    assert_eq!(target, "bl-1e60");
}
