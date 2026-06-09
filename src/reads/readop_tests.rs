//! Tests for the §6 read-op plugin dispatch: the fold is best-effort — every
//! failure mode renders an empty contribution (the read still prints, minus the
//! line) but lands an `error` record — and a wired plugin's captured stdout
//! comes back verbatim, carrying the §7 read wire (bare-key hook, `read`
//! phase, `metadata.bl-id` for `show`, no id for the target-free reads).
//! Plugin stderr is enveloped at `info`; core's `invoke` narration is `debug`.

use std::path::Path;

use tempfile::TempDir;

use super::super::test_support::{bind_script, edge, landing_with, log_at, log_lines};
use super::fold;
use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::log::Level;
use crate::plugin::DEPTH_CAP;
use crate::registry::Registry;
use crate::verb::Verb;

/// [`fold`] with the op-constant collaborators defaulted: a default config and
/// a debug-threshold log sink in `tmp` (read back via [`log_lines`]).
fn fold_at(tmp: &Path, edge: &Edge, verb: Verb, id: Option<&str>) -> String {
    fold(edge, tmp, verb, id, &EffectiveConfig::default(), &log_at(tmp, Level::Debug, verb))
}

const SHOW_WIRED: &str = "[hooks]\n\"show\" = [\"fake\"]\n";

#[test]
fn fold_captures_a_wired_plugins_stdout_verbatim() {
    let tmp = TempDir::new().unwrap();
    let edge = edge(tmp.path(), 0);
    let landing = landing_with(&edge, SHOW_WIRED);
    // The script proves the §6/§7 read contract end to end: argv is `<op> read`,
    // and the wire carries the named ball as `metadata.bl-id` — then prints the
    // one line the fold captures.
    bind_script(
        tmp.path(),
        &landing,
        "fake",
        "#!/bin/sh\npayload=$(cat)\ncase \"$payload\" in *'\"bl-id\":[\"bl-7\"]'*) ;; *) exit 1;; esac\necho \"line $1 $2 $BALLS_PLUGIN_NAME\"\n",
    );
    assert_eq!(fold_at(tmp.path(), &edge, Verb::Show, Some("bl-7")), "line show read fake\n");
    // Core narrated the invoke — at `debug`, like all read-op narration (§4).
    assert!(log_lines(tmp.path()).contains(r#""lvl":"debug","src":"core","op":"show","msg":"invoke fake""#));
}

#[test]
fn a_target_free_read_dispatches_with_empty_metadata() {
    // `list` names no ball (§6): the wire's metadata carries no
    // `bl-id` — the same dispatch, minus the id channel.
    let tmp = TempDir::new().unwrap();
    let edge = edge(tmp.path(), 0);
    let landing = landing_with(&edge, "[hooks]\n\"list\" = [\"fake\"]\n");
    bind_script(
        tmp.path(),
        &landing,
        "fake",
        "#!/bin/sh\npayload=$(cat)\ncase \"$payload\" in *bl-id*) exit 1;; esac\necho \"bare $1 $2\"\n",
    );
    assert_eq!(fold_at(tmp.path(), &edge, Verb::List, None), "bare list read\n");
}

#[test]
fn fold_skips_a_failing_plugin_but_keeps_the_others_lines() {
    // Non-fatal by §6: a non-zero exit drops THAT plugin's contribution; the
    // chain's other lines still fold, in list order — and the failure locus
    // lands as an `error` record, surviving any threshold.
    let tmp = TempDir::new().unwrap();
    let edge = edge(tmp.path(), 0);
    let landing = landing_with(&edge, "[hooks]\n\"show\" = [\"bad\", \"good\"]\n");
    bind_script(tmp.path(), &landing, "bad", "#!/bin/sh\ncat > /dev/null\necho noise\nexit 3\n");
    bind_script(tmp.path(), &landing, "good", "#!/bin/sh\ncat > /dev/null\necho kept\n");
    assert_eq!(fold_at(tmp.path(), &edge, Verb::Show, Some("bl-1")), "kept\n");
    assert!(log_lines(tmp.path())
        .contains(r#""lvl":"error","src":"core","op":"show","msg":"plugin bad failed the show read dispatch""#));
}

#[test]
fn plugin_stderr_is_enveloped_at_info() {
    // §6: balls pipes the child's stderr and envelopes each line into the op
    // log (`src=<name>`, `lvl=info`) — on a read exactly as on a mutating op,
    // so it lands even at the default threshold (which drops the `debug`
    // invoke narration).
    let tmp = TempDir::new().unwrap();
    let edge = edge(tmp.path(), 0);
    let landing = landing_with(&edge, SHOW_WIRED);
    bind_script(tmp.path(), &landing, "fake", "#!/bin/sh\ncat > /dev/null\necho diagnostic >&2\necho line\n");
    let log = log_at(tmp.path(), Level::Info, Verb::Show);
    let out = fold(&edge, tmp.path(), Verb::Show, Some("bl-1"), &EffectiveConfig::default(), &log);
    assert_eq!(out, "line\n");
    let lines = log_lines(tmp.path());
    assert!(lines.contains(r#""lvl":"info","src":"fake","op":"show","msg":"diagnostic""#));
    assert!(!lines.contains("invoke")); // debug narration stays below `info`
}

#[test]
fn fold_is_empty_at_the_recursion_cap() {
    // At the §6 depth cap no further plugin may spawn — the read renders bare.
    let tmp = TempDir::new().unwrap();
    assert_eq!(fold_at(tmp.path(), &edge(tmp.path(), DEPTH_CAP), Verb::Show, Some("bl-1")), "");
}

#[test]
fn fold_is_empty_when_nothing_is_wired_or_the_schedule_is_malformed() {
    // No landing at all → an empty schedule → nothing to run.
    let tmp = TempDir::new().unwrap();
    assert_eq!(fold_at(tmp.path(), &edge(tmp.path(), 0), Verb::Show, Some("bl-1")), "");
    // A malformed plugins.toml is non-fatal on a read: fold contributes nothing.
    let broken = TempDir::new().unwrap();
    let e = edge(broken.path(), 0);
    landing_with(&e, "[hooks");
    assert_eq!(fold_at(broken.path(), &e, Verb::Show, Some("bl-1")), "");
}

#[test]
fn fold_skips_a_dangling_binding_and_an_unspawnable_binary() {
    // Wired but not bound (`bin/<name>` absent) — skipped, not an abort…
    let tmp = TempDir::new().unwrap();
    let e = edge(tmp.path(), 0);
    landing_with(&e, SHOW_WIRED);
    assert_eq!(fold_at(tmp.path(), &e, Verb::Show, Some("bl-1")), "");
    // …and a bound but non-executable file fails to spawn — skipped too, but
    // narrated at `error` like any other failed contribution (§6).
    let tmp2 = TempDir::new().unwrap();
    let e2 = edge(tmp2.path(), 0);
    let landing2 = landing_with(&e2, SHOW_WIRED);
    let bin = tmp2.path().join("fake");
    std::fs::write(&bin, "not executable").unwrap();
    Registry::at(&landing2).bind("fake", &bin).unwrap();
    assert_eq!(fold_at(tmp2.path(), &e2, Verb::Show, Some("bl-1")), "");
    assert!(log_lines(tmp2.path()).contains(r#""lvl":"error","src":"core""#));
}
