//! `update --edit` tests — the [`Editor`] seam refusals, the editor loop
//! (no-op / accept / re-edit / abort), and the end-to-end seal through
//! [`crate::mutate`]'s dispatch with a scripted editor (`sh <script>` — exec'ing
//! `sh`, not a freshly written file, so no ETXTBSY race in parallel runs).

use super::*;
use crate::edge::Edge;
use crate::layout;
use crate::taskfile::{read_task, task_ids};
use crate::verb::Verb;
use tempfile::TempDir;

/// A stored-ball stand-in for the direct [`Editor::edited`] tests.
fn fixture() -> Task {
    Task { title: "A task".into(), created: 100, updated: 100, body: "body\n".into(), ..Task::default() }
}

/// An [`Editor`] with `tty` up and `input` at EOF — the common scripted shape.
fn scripted(cmd: &str) -> Editor {
    Editor { cmd: Some(cmd.into()), tty: true, input: Box::new(io::empty()) }
}

#[test]
fn edit_requires_a_tty_and_a_usable_editor() {
    // Non-tty (the agent invocation) refuses outright — flags stay the agent path.
    let mut no_tty = Editor { cmd: Some("vi".into()), tty: false, input: Box::new(io::empty()) };
    assert!(no_tty.edited(&fixture(), "bl-1").unwrap_err().to_string().contains("not a tty"));
    // Unset $EDITOR/$VISUAL is an error, not a fallback.
    let mut unset = Editor { cmd: None, tty: true, input: Box::new(io::empty()) };
    assert!(unset.edited(&fixture(), "bl-1").unwrap_err().to_string().contains("set $EDITOR"));
    // A whitespace-only $EDITOR has no program to run.
    assert!(scripted("  ").edited(&fixture(), "bl-1").unwrap_err().to_string().contains("is empty"));
    // An editor that exits non-zero aborts the op (the buffer is suspect).
    assert!(scripted("false").edited(&fixture(), "bl-1").unwrap_err().to_string().contains("exited"));
}

#[test]
fn an_untouched_buffer_is_the_idempotent_noop() {
    // `true` exits clean without touching the buffer: parses back equal ⇒ None.
    assert!(scripted("true").edited(&fixture(), "bl-1").unwrap().is_none());
}

#[test]
fn an_edited_buffer_parses_back_as_the_new_task() {
    let tmp = TempDir::new().unwrap();
    let script = tmp.path().join("editor.sh");
    fs::write(&script, "printf '+++\\ntitle = \"Edited\"\\ncreated = 1\\nupdated = 1\\n+++\\nnew\\n' > \"$1\"\n").unwrap();
    let after = scripted(&format!("sh {}", script.display())).edited(&fixture(), "bl-1").unwrap().unwrap();
    assert_eq!(after.title, "Edited");
    assert_eq!(after.body, "new\n");
}

#[test]
fn a_rejected_buffer_offers_a_reedit_pass_over_the_same_buffer() {
    let tmp = TempDir::new().unwrap();
    let (script, flag) = (tmp.path().join("editor.sh"), tmp.path().join("second-pass"));
    // First pass writes garbage; the re-edit pass (flag file present) fixes it.
    fs::write(
        &script,
        "if [ -e \"$1\" ]; then printf '+++\\ntitle = \"Fixed\"\\ncreated = 0\\nupdated = 0\\n+++\\n' > \"$2\"; \
         else : > \"$1\"; printf 'garbage' > \"$2\"; fi\n",
    )
    .unwrap();
    let mut editor = Editor {
        cmd: Some(format!("sh {} {}", script.display(), flag.display())),
        tty: true,
        input: Box::new(io::Cursor::new(b"y\n".to_vec())),
    };
    let after = editor.edited(&fixture(), "bl-1").unwrap().unwrap();
    assert_eq!(after.title, "Fixed");
}

#[test]
fn a_rejected_buffer_aborts_without_an_explicit_yes() {
    let tmp = TempDir::new().unwrap();
    let script = tmp.path().join("editor.sh");
    fs::write(&script, "printf 'garbage' > \"$1\"\n").unwrap();
    // EOF on the prompt (no `y`) aborts: garbage is NEVER committed.
    let err = scripted(&format!("sh {}", script.display())).edited(&fixture(), "bl-1").unwrap_err();
    assert!(err.to_string().contains("rejected"), "{err}");
    assert!(err.to_string().contains("frontmatter"), "{err}");
}

/// An edge rooted in `tmp` (stealth: no plugin binaries, empty chains) — the
/// same shape as the dispatch tests in [`crate::tests`].
fn edged(tmp: &TempDir) -> Edge {
    Edge {
        xdg: layout::Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir: None,
        color: false,
        log_level: None,
    }
}

#[test]
fn edit_seals_a_replaced_buffer_and_noops_on_an_unchanged_one() {
    let tmp = TempDir::new().unwrap();
    let edge = edged(&tmp);
    let run = |args: &[&str]| crate::run(&edge, &args.iter().map(ToString::to_string).collect::<Vec<_>>());
    assert_eq!(run(&["prime", "--as", "me"]), 0);
    assert_eq!(run(&["create", "A task", "--as", "me", "--body", "old"]), 0);
    let store = edge.xdg.clone_dir(&edge.invocation_path).store();
    let id = task_ids(&store).unwrap().pop().unwrap();
    let before = read_task(&store, &id).unwrap();
    let args: Vec<String> = vec![id.clone(), "--edit".into(), "--as".into(), "me".into()];

    // Unchanged buffer: the no-op leaves the store ball untouched — nothing sealed.
    super::super::dispatch(&edge, Verb::Update, &args, &mut scripted("true")).unwrap();
    assert_eq!(read_task(&store, &id).unwrap(), before);

    // A rewritten buffer seals through the EXISTING update path: title/body
    // replaced, `created` PRESERVED (the hand-typed 1 is overwritten), `updated`
    // restamped by the seal (never the hand-typed value).
    let script = tmp.path().join("editor.sh");
    fs::write(&script, "printf '+++\\ntitle = \"Edited\"\\ncreated = 1\\nupdated = 1\\n+++\\nnew\\n' > \"$1\"\n").unwrap();
    super::super::dispatch(&edge, Verb::Update, &args, &mut scripted(&format!("sh {}", script.display()))).unwrap();
    let after = read_task(&store, &id).unwrap();
    assert_eq!(after.title, "Edited");
    assert_eq!(after.body, "new\n");
    assert_eq!(after.created, before.created);
    assert!(after.updated >= before.updated, "the seal restamps updated");
}
