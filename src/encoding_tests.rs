//! Tests for the encoding layer. Frozen golden vectors for §14.2 —
//! the URL/encoding contract is the SPEC §10 hand-operable promise,
//! so changing any output here is a SPEC-level break.

use super::*;
use std::path::PathBuf;

#[test]
fn canonicalize_strips_https_scheme() {
    assert_eq!(
        canonicalize_origin("https://github.com/mudbungie/balls.git"),
        "github.com/mudbungie/balls"
    );
}

#[test]
fn canonicalize_strips_http_scheme() {
    assert_eq!(
        canonicalize_origin("http://example.com/foo"),
        "example.com/foo"
    );
}

#[test]
fn canonicalize_preserves_no_scheme() {
    // SSH-style URLs have no scheme; canonical form drops nothing on
    // step 1.
    assert_eq!(
        canonicalize_origin("git@github.com:mudbungie/balls.git"),
        "github.com:mudbungie/balls"
    );
}

#[test]
fn canonicalize_strips_userinfo() {
    assert_eq!(canonicalize_origin("git@github.com:foo/bar"), "github.com:foo/bar");
    assert_eq!(canonicalize_origin("user:pass@host.com/path"), "host.com/path");
}

#[test]
fn canonicalize_strips_dot_git_suffix() {
    assert_eq!(
        canonicalize_origin("github.com:foo/bar.git"),
        "github.com:foo/bar"
    );
}

#[test]
fn canonicalize_strips_trailing_slash() {
    assert_eq!(
        canonicalize_origin("github.com:foo/bar/"),
        "github.com:foo/bar"
    );
}

#[test]
fn canonicalize_lowercases_ascii() {
    // The lowercase pass runs LAST per SPEC §10. So `.GIT` does NOT
    // strip (the `.git$` regex is case-sensitive); we end up with a
    // trailing `.git` in the lowercased output. A user with uppercase
    // `.GIT` in their git config gets a different canonical form
    // than one with `.git` — documented edge that motivates the §13
    // "URL alias drift" non-goal and the `git remote set-url`
    // recommendation.
    assert_eq!(
        canonicalize_origin("GitHub.com:Foo/Bar.GIT"),
        "github.com:foo/bar.git"
    );
    // The common case: lowercase `.git` strips correctly.
    assert_eq!(
        canonicalize_origin("GitHub.com:Foo/Bar.git"),
        "github.com:foo/bar"
    );
}

#[test]
fn canonicalize_idempotent_when_already_lowercase() {
    // The §10 sequence is idempotent on already-canonicalized inputs
    // (no scheme, no @, no `.git`, no trailing /, already lowercase).
    // That's the operational property: bl's outputs feed back as
    // inputs in any storage/round-trip path without drift.
    let s = "github.com/foo/bar";
    assert_eq!(canonicalize_origin(s), s);
    assert_eq!(canonicalize_origin(&canonicalize_origin(s)), s);
}

#[test]
fn canonicalize_not_idempotent_on_uppercase_input() {
    // Counter-example: §10's strips happen before lowercase, so a
    // first pass on `https://X/y.git/` strips the scheme + trailing
    // slash + (no .git strip since X.git is still uppercase before
    // the case-sensitive strip) and produces `x/y.git` after the
    // final lowercase. A SECOND pass then strips the now-lowercase
    // `.git`. Document this so a future refactor doesn't quietly add
    // a "normalize idempotent" branch — the §10 hand-operable
    // sequence is the contract, idempotency is not.
    //
    // The scheme strip uses `[a-z]+` — it matches lowercase only.
    // So `https://` strips on pass 1; an uppercase scheme `HTTPS://`
    // would NOT strip (see `golden_vector_mixed_case_and_trailing_slash`).
    let once = canonicalize_origin("https://Github.com/foo/bar.GIT/");
    // Pass 1: strip "https://" → "Github.com/foo/bar.GIT/"
    //         no @, no .git (case mismatch), strip / → "Github.com/foo/bar.GIT"
    //         lowercase → "github.com/foo/bar.git"
    assert_eq!(once, "github.com/foo/bar.git");
    let twice = canonicalize_origin(&once);
    // Pass 2: no scheme, no @, .git strips → "github.com/foo/bar"
    assert_eq!(twice, "github.com/foo/bar");
    assert_ne!(once, twice);
}

#[test]
fn canonicalize_empty_string_passes_through() {
    assert_eq!(canonicalize_origin(""), "");
}

#[test]
fn canonicalize_no_scheme_with_at_sign_in_path_only() {
    // Path-style URLs with no @host: the at-strip would take everything
    // up to and including @. SPEC §4 says userinfo strip is `[^@]+@`
    // applied AFTER the scheme strip — the only @ is userinfo. Accept
    // this corner: tests cover the URL shapes balls actually sees.
    assert_eq!(canonicalize_origin("foo@bar"), "bar");
}

#[test]
fn canonicalize_aliasing_collision_gh_case_and_git_suffix() {
    // Two URLs that *should* collide per SPEC §14.2's "two URLs that
    // should collide must encode to the same value." `Foo.git` and
    // `foo` after canonicalize → both `foo`.
    assert_eq!(canonicalize_origin("Foo.git"), canonicalize_origin("foo"));
}

#[test]
fn percent_encode_preserves_unreserved() {
    // RFC 3986 §2.3 unreserved set must pass through unchanged.
    let s = "ABCdef123-._~";
    assert_eq!(percent_encode_component(s), s);
}

#[test]
fn percent_encode_encodes_slash() {
    assert_eq!(percent_encode_component("a/b"), "a%2Fb");
}

#[test]
fn percent_encode_encodes_colon_at_question_hash_amp_space() {
    assert_eq!(percent_encode_component(":"), "%3A");
    assert_eq!(percent_encode_component("@"), "%40");
    assert_eq!(percent_encode_component("?"), "%3F");
    assert_eq!(percent_encode_component("#"), "%23");
    assert_eq!(percent_encode_component("&"), "%26");
    assert_eq!(percent_encode_component(" "), "%20");
}

#[test]
fn percent_encode_uses_uppercase_hex() {
    // SPEC §4: percent-encoding produces `%XX` with uppercase hex
    // digits. 0xff → %FF, not %ff. This matches `jq -Rr @uri`.
    assert_eq!(percent_encode_component("\u{ff}"), "%C3%BF");
    assert_eq!(percent_encode_component(":"), "%3A"); // not %3a
}

#[test]
fn percent_encode_encodes_non_ascii_as_utf8_bytes() {
    // Multi-byte UTF-8 sequences are encoded byte-by-byte (matches
    // `jq -Rr @uri`).
    assert_eq!(percent_encode_component("é"), "%C3%A9"); // U+00E9
    assert_eq!(percent_encode_component("中"), "%E4%B8%AD"); // U+4E2D
}

#[test]
fn percent_encode_output_has_no_slashes() {
    // SPEC §4: "the result is reversible and contains no slashes, so
    // the URL becomes one component." Stress: an input full of slashes
    // and dots must not produce a path component with `/` or `..`.
    let out = percent_encode_component("../foo/bar/../baz");
    // `..` survives literally because `.` is unreserved — but it
    // cannot escape because the slashes are gone.
    assert!(!out.contains('/'), "{out} contains /");
}

#[test]
fn percent_encode_idempotent_after_first_encode() {
    // Encoding twice further encodes the `%` from round 1. Not
    // idempotent — by design, since `jq -Rr @uri` is not idempotent
    // either. Lock this so a future refactor doesn't quietly add an
    // "idempotent" branch.
    let once = percent_encode_component(":");
    let twice = percent_encode_component(&once);
    assert_eq!(once, "%3A");
    assert_eq!(twice, "%253A");
}

#[test]
fn percent_encode_empty_string() {
    assert_eq!(percent_encode_component(""), "");
}

#[test]
fn enc_balls_tasks_constant_matches_percent_encoder() {
    // The compile-time constant must match the runtime encoder. If
    // they ever drift, the new layout breaks silently.
    assert_eq!(ENC_BALLS_TASKS, percent_encode_component("balls/tasks"));
    assert_eq!(ENC_BALLS_TASKS, "balls%2Ftasks");
}

// --- Frozen golden vectors per SPEC §14.2 ---
//
// These vectors are the contract. A change here is a SPEC-level break
// that requires updating the documented hand-operable sequence (§10).

#[test]
fn golden_vector_github_ssh() {
    let url = "git@github.com:mudbungie/balls.git";
    let canonical = canonicalize_origin(url);
    let encoded = percent_encode_component(&canonical);
    assert_eq!(canonical, "github.com:mudbungie/balls");
    assert_eq!(encoded, "github.com%3Amudbungie%2Fballs");
}

#[test]
fn golden_vector_github_https() {
    let url = "https://github.com/mudbungie/balls.git";
    let canonical = canonicalize_origin(url);
    let encoded = percent_encode_component(&canonical);
    assert_eq!(canonical, "github.com/mudbungie/balls");
    assert_eq!(encoded, "github.com%2Fmudbungie%2Fballs");
}

#[test]
fn golden_vector_gitlab_https_no_dot_git() {
    let url = "https://gitlab.com/group/project";
    let canonical = canonicalize_origin(url);
    let encoded = percent_encode_component(&canonical);
    assert_eq!(canonical, "gitlab.com/group/project");
    assert_eq!(encoded, "gitlab.com%2Fgroup%2Fproject");
}

#[test]
fn golden_vector_mixed_case_and_trailing_slash() {
    // The §10 scheme strip is `[a-z]+://` — uppercase `HTTPS://`
    // does not match, so it survives steps 1-4 and only gets
    // lowercased by step 5. The trailing `.git` survives because
    // the strip ran while the suffix was still `.git/` (not
    // matching `.git$`). §13 names URL alias drift as a non-goal —
    // run `git remote set-url` to normalize.
    let canonical = canonicalize_origin("HTTPS://GitHub.com/Foo/Bar.git/");
    assert_eq!(canonical, "https://github.com/foo/bar.git");
    assert_eq!(
        percent_encode_component(&canonical),
        "https%3A%2F%2Fgithub.com%2Ffoo%2Fbar.git"
    );
}

#[test]
fn nested_clone_path_strips_leading_slash() {
    let p = nested_clone_path(&PathBuf::from("/home/mark/dev/balls"));
    assert_eq!(p, PathBuf::from("home/mark/dev/balls"));
}

#[test]
fn nested_clone_path_root_path() {
    // Edge: just "/" → "". Treat as a degenerate case but don't panic.
    let p = nested_clone_path(&PathBuf::from("/"));
    assert_eq!(p, PathBuf::from(""));
}

#[test]
fn nested_clone_path_relative_passes_through() {
    // Non-absolute path passes through unchanged. Callers feed
    // absolute paths in production; this guard is for defensive
    // robustness, not a contract.
    let p = nested_clone_path(&PathBuf::from("home/mark/dev/balls"));
    assert_eq!(p, PathBuf::from("home/mark/dev/balls"));
}

#[test]
fn nested_clone_path_preserves_internal_slashes() {
    let p = nested_clone_path(&PathBuf::from("/a/b/c/d/e"));
    assert_eq!(p, PathBuf::from("a/b/c/d/e"));
}

#[test]
fn nested_clone_path_with_trailing_slash() {
    // Don't strip the trailing slash; that's a job for downstream
    // PathBuf normalization.
    let p = nested_clone_path(&PathBuf::from("/home/mark/"));
    assert_eq!(p, PathBuf::from("home/mark/"));
}
