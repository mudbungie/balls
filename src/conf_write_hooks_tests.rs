//! `bl conf` writes to the `[hooks]` schedule (bl-c2de) — the §4 list compose
//! (append/prepend/remove convergence), the bare `set` whole-list replace, the
//! non-empty plugin-name guard (bl-bee0), foreign-table round-tripping, the
//! wired dead-verb key (bl-03a1 read/remove-but-not-create), and the non-table
//! `[hooks]` refusal. Shares the parent module's edge/founded/conf/landing_text
//! fixtures via [`super`].

use super::*;

#[test]
fn hook_compose_applies_the_directive_and_converges() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    conf(&e, &["append", "close.pre", "x"]).unwrap();
    assert!(landing_text(&clone, "plugins.toml").contains("\"close.pre\" = [\"x\"]"));
    let sealed = commits(&clone.landing());
    // A present name re-appended (or re-prepended) is the convergent no-op.
    conf(&e, &["append", "close.pre", "x"]).unwrap();
    conf(&e, &["prepend", "close.pre", "x"]).unwrap();
    assert_eq!(commits(&clone.landing()), sealed);
    conf(&e, &["prepend", "close.pre", "y"]).unwrap();
    assert!(landing_text(&clone, "plugins.toml").contains("\"close.pre\" = [\"y\", \"x\"]"));
    // Removing an absent name converges too; removing the last drops the key.
    conf(&e, &["remove", "close.pre", "ghost"]).unwrap();
    conf(&e, &["remove", "close.pre", "y"]).unwrap();
    conf(&e, &["remove", "close.pre", "x"]).unwrap();
    assert!(!landing_text(&clone, "plugins.toml").contains("close.pre"));
}

#[test]
fn set_on_a_hooks_key_bare_replaces_the_whole_list() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    conf(&e, &["set", "close.pre", "a", "b"]).unwrap();
    assert!(landing_text(&clone, "plugins.toml").contains("\"close.pre\" = [\"a\", \"b\"]"));
    // Replacing with nothing empties the list — the key drops (§4 absent/empty).
    conf(&e, &["set", "close.pre"]).unwrap();
    assert!(!landing_text(&clone, "plugins.toml").contains("close.pre"));
}

#[test]
fn an_empty_plugin_name_is_refused_and_writes_nothing() {
    // bl-bee0: `set <hooks-key> ""` wrote [""], which dispatch later resolved
    // to bin/ itself (EACCES). A plugin name is non-empty — clearing the list
    // already has its spelling (`set <key>` with no values). Refused at the
    // front door (the bl-ac89 precedent), nothing written, nothing sealed.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    let before = commits(&clone.landing());
    for argv in [
        vec!["set", "close.pre", ""],
        vec!["set", "close.pre", "a", ""],
        vec!["append", "close.pre", ""],
        vec!["prepend", "close.pre", ""],
        vec!["remove", "close.pre", ""],
    ] {
        let err = conf(&e, &argv).unwrap_err().to_string();
        assert!(err.contains("non-empty"), "{argv:?}: {err}");
    }
    assert!(!landing_text(&clone, "plugins.toml").contains("close.pre"));
    assert_eq!(commits(&clone.landing()), before);
}

#[test]
fn foreign_tables_round_trip_untouched() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    fs::write(
        clone.landing().join("config").join("plugins.toml"),
        "[team]\nkeep = \"this\"\n\n[hooks]\n\"close.pre\" = [\"a\"]\n",
    )
    .unwrap();
    conf(&e, &["append", "close.pre", "b"]).unwrap();
    let body = landing_text(&clone, "plugins.toml");
    assert!(body.contains("keep = \"this\""), "{body}");
    assert!(body.contains("\"close.pre\" = [\"a\", \"b\"]"), "{body}");
}

#[test]
fn a_wired_retired_verb_hook_is_readable_and_removable_but_uncreatable() {
    // bl-03a1: the dump surfaces every wired `[hooks]` key; the per-key path
    // once rejected any whose op was not a live verb — so a default-seeded /
    // stale `drop.post` (the `drop` verb is gone) showed in the dump yet failed
    // read/set/remove with `unknown key`, clearable only by hand-editing the
    // landing. Now what the dump shows, the per-key path operates on: read it,
    // and REMOVE it (the workaround the bug filer wanted). A token the schedule
    // does NOT wire is still refused — a typo can't mint wiring for a dead verb.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    fs::write(
        clone.landing().join("config").join("plugins.toml"),
        "[hooks]\n\"drop.post\" = [\"bl-delivery\"]\n",
    )
    .unwrap();
    conf(&e, &["drop.post"]).unwrap(); // read the wired key — no `unknown key`
    conf(&e, &["remove", "drop.post", "bl-delivery"]).unwrap(); // and clear it
    assert!(!landing_text(&clone, "plugins.toml").contains("drop.post"));
    // Absent from the schedule, the same dead-verb token is refused for create.
    let err = conf(&e, &["set", "drop.post", "bl-tracker"]).unwrap_err().to_string();
    assert!(err.contains("unknown key"), "{err}");
}

#[test]
fn a_non_table_hooks_root_is_refused() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    fs::write(clone.landing().join("config").join("plugins.toml"), "hooks = \"nope\"\n").unwrap();
    let err = conf(&e, &["append", "close.pre", "x"]).unwrap_err().to_string();
    assert!(err.contains("[hooks] is not a table"), "{err}");
}
