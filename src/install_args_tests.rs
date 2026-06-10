//! Tests for `bl install`'s argv parse — the §6 defaults (`path`, `--from`,
//! `--to`) and the parse-time direction guards.

use super::*;

fn parsed(args: &[&str]) -> io::Result<Opts> {
    parse(&args.iter().map(ToString::to_string).collect::<Vec<_>>(), "tester")
}

#[test]
fn parse_defaults_the_path_the_from_and_the_to_ref() {
    // Bare `bl install` = the recommended bundle, from the configured upstream
    // (left `None` for the run wiring to fetch), onto the landing (§6).
    let o = parsed(&[]).unwrap();
    assert_eq!((o.path.as_str(), o.from, o.to.as_str()), (DEFAULT_PATH, None, LANDING_BRANCH));
    assert_eq!(o.actor.as_str(), "tester");
    assert!(o.bins.is_empty());
}

#[test]
fn parse_takes_an_explicit_path_refs_actor_and_bins() {
    let o = parsed(&["tasks/*", "--from", "a", "--to", "b", "--as", "me"]).unwrap();
    assert_eq!((o.path.as_str(), o.from.as_deref()), ("tasks/*", Some("a")));
    assert_eq!((o.to.as_str(), o.actor.as_str()), ("b", "me"));

    // `--bin <name>=<path>` repeats, one explicit candidate per plugin; the
    // path half may itself carry `=`.
    let o = parsed(&["--from", "x", "--bin", "jira=/opt/jira", "--bin", "t=/v=1/t"]).unwrap();
    assert_eq!(o.bins["jira"], PathBuf::from("/opt/jira"));
    assert_eq!(o.bins["t"], PathBuf::from("/v=1/t"));
}

#[test]
fn parse_rejects_bad_shapes() {
    for (args, needle) in [
        (&["a", "b", "--from", "x"][..], "at most one path"),
        (&["--from", "x", "--frobnicate"][..], "unexpected flag"),
        (&["/etc/config", "--from", "x"][..], "checkout-relative"),
        (&["config/../tasks", "--from", "x"][..], "checkout-relative"),
        (&["--from"][..], "--from needs a value"),
        (&["--bin"][..], "--bin needs a value"),
        (&["--bin", "tracker"][..], "<name>=<path>"),
        // The §6 defaults and `--bin` belong to the landing-targeted direction;
        // an explicit flag silently dropped would be the bl-cf93 sin.
        (&["--to", "balls/tasks"][..], "--from is required when --to"),
        (&["--from", "x", "--to", "balls/tasks", "--bin", "t=/x"][..], "landing-targeted"),
    ] {
        let err = parsed(args).unwrap_err();
        assert!(err.to_string().contains(needle), "{args:?}: {err}");
    }
}
