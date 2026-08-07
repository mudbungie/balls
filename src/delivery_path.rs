//! §11 delivery path/string derivations — the pure `(binding, id)` → worktree
//! path / branch / subject / marker arithmetic, split out of [`crate::delivery`]
//! so the policy module holds only the hook matrix. No git, no IO beyond path
//! math; balls prints the same paths from the same formulas (no return channel).

use std::path::{Component, Path, PathBuf};

use crate::layout::Xdg;

/// This binding's worktree territory (§11):
/// `$XDG_STATE_HOME/balls/plugins/<name>/<invocation_path>/`. Every `work/<id>`
/// worktree is an `<id>/` child; [`worktree_path`] joins one id onto it.
///
/// Unlike every other layout name (which percent-encodes its key into one
/// inspectable component, §1), this one MIRRORS the invocation path verbatim —
/// the leading `/` stripped so it nests rather than re-roots. The reason is
/// concrete: this subtree is the project's *code* worktree, where `cargo`/`rustc`
/// build, and `rust-lld` cannot open an output file whose path contains a `%`
/// (bl-f3e4). A percent-encoded ancestor would poison every link. Mirroring the
/// real path is at least as inspectable as encoding it (§1's actual goal — names
/// you can read, never a hash) and is always a valid filesystem path, since the
/// invocation path already is one. The git-data layouts (clones, tracker) keep
/// percent-encoding: nothing compiles there, so `%` is harmless.
#[must_use]
pub fn binding_territory(xdg: &Xdg, plugin: &str, invocation_path: &str) -> PathBuf {
    xdg.plugin_territory(plugin).join(invocation_path.trim_start_matches('/'))
}

/// Reject an `invocation_path` that is not a clean absolute path, BEFORE it is
/// mirrored by [`binding_territory`] (bl-2d6d). The mirror joins the path
/// verbatim — it gives up the `..`-neutralization percent-encoding gives the
/// clone layout — so a relative path or a `..` component would let the worktree
/// escape plugin territory. The delivery edge calls this once, at wire ingress,
/// before any worktree path is derived.
pub fn ensure_safe_invocation_path(p: &str) -> std::io::Result<()> {
    let path = Path::new(p);
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(std::io::Error::other(format!(
            "unsafe invocation path (must be absolute, no '..'): {p:?}"
        )));
    }
    Ok(())
}

/// The derived code-worktree path (§11): the `<id>/` child of this binding's
/// [`binding_territory`]. balls prints the same path from the same formula — no
/// return channel. Pairs with [`work_branch`] — both derive from the same `<id>`
/// key, so §11 claimant-keying (`<key> = <id>` or `<id>-<claimant>`) is a single
/// edit across the pair, not a hunt for every `work/<id>` literal.
#[must_use]
pub fn worktree_path(xdg: &Xdg, plugin: &str, invocation_path: &str, id: &str) -> PathBuf {
    binding_territory(xdg, plugin, invocation_path).join(id)
}

/// The `work/<id>` branch this binding's worktree sits on (§11) — the BRANCH
/// half of the `(worktree_path, branch)` pair. Every site that derives one must
/// derive the other through these two helpers so they cannot drift; see
/// [`worktree_path`].
#[must_use]
pub fn work_branch(id: &str) -> String {
    format!("work/{id}")
}

/// An ATTEMPT's private worktree (§11.1, bl-4eac):
/// `$XDG_STATE_HOME/balls/attempts/<invocation_path>/<handle>/`. A sibling
/// territory of the delivery plugin's, not a child of it — an attempt is a
/// capability of the crate, not an op of one plugin binding, so it cannot key on
/// a plugin name. The invocation path is MIRRORED verbatim for the same reason
/// [`binding_territory`] mirrors it: this is a cargo build dir and `rust-lld`
/// cannot open an output file under a `%` ancestor (bl-f3e4). Pairs with
/// [`attempt_branch`] on the same `<handle>` key, exactly as
/// [`worktree_path`] pairs with [`work_branch`] on `<id>`.
#[must_use]
pub fn attempt_path(xdg: &Xdg, invocation_path: &str, handle: &str) -> PathBuf {
    xdg.state_dir().join("attempts").join(invocation_path.trim_start_matches('/')).join(handle)
}

/// The `attempt/<handle>` source ref of one attempt (§11.1, bl-4eac) — a
/// namespace DISTINCT from `work/*`, which remains ball identity. Two
/// consequences ride the separation and neither needs a flag: `prime`'s settled-
/// branch prune globs `work/` and so leaves attempts alone (retention is the
/// caller's), and a `git branch --list 'attempt/*'` is the whole enumeration of
/// live attempts.
#[must_use]
pub fn attempt_branch(handle: &str) -> String {
    format!("attempt/{handle}")
}

/// The delivery commit subject: `<title> [<id>]`. The `[<id>]` tag is delivery
/// ground truth — the `marked` tag-scan (§11) reads the integration branch for
/// it, and deliver's retry standing detects a landed squash by it.
#[must_use]
pub fn subject(title: &str, id: &str) -> String {
    format!("{title} [{id}]")
}

/// `[<id>]` — the delivery tag the squash subject carries and the retry
/// standing / `marked` tag-scan greps for.
#[must_use]
pub fn marker(id: &str) -> String {
    format!("[{id}]")
}

#[cfg(test)]
#[path = "delivery_path_tests.rs"]
mod tests;
