//! Legacy on-disk layout detection per [SPEC-clone-layout.md] §11.1 +
//! Phase 3 (bl-05e5).
//!
//! One source of truth for "is this clone still on the pre-XDG
//! layout?" — used by `bl prime`'s legacy warning (§12 row 2), by
//! `bl doctor`'s legacy-layout report, and by `bl migrate`'s
//! `Detection::Legacy` gate. Each call lists the specific markers
//! found so the user-facing warning can name them by path rather than
//! emit a generic "legacy layout in use" line.
//!
//! The hashed-XDG layout (bl-fc50) was reverted before any release
//! shipped and so cannot exist in the wild (see `migrate.rs` for the
//! scope-reduction note). The bl-ed32 `workspace.json` shape was a
//! SPEC-only artifact, never written by any binary. Therefore the
//! only marker this module looks for is the pre-XDG one:
//! `.balls/config.json` committed on the code branch.
//!
//! [SPEC-clone-layout.md]: ../docs/SPEC-clone-layout.md

use std::path::{Path, PathBuf};

/// Each marker is one piece of evidence that a clone still carries the
/// pre-XDG layout. The path is included so the warning/report can name
/// it explicitly instead of just saying "legacy layout in use."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyMarker {
    /// One-line human-readable description, e.g.
    /// `"pre-XDG .balls/config.json on the code branch"`.
    pub kind: &'static str,
    /// Absolute path of the marker on disk.
    pub path: PathBuf,
}

/// Inspect `clone_root` and return every legacy-layout marker found.
/// An empty vector means the clone is on the XDG layout (or has no
/// balls state at all — callers distinguish that elsewhere).
///
/// Pure file-system probe, no git ops, no I/O beyond `Path::exists()`.
#[must_use]
pub fn detect(clone_root: &Path) -> Vec<LegacyMarker> {
    let mut markers = Vec::new();
    let pre_xdg = clone_root.join(".balls/config.json");
    if pre_xdg.exists() {
        markers.push(LegacyMarker {
            kind: "pre-XDG .balls/config.json on the code branch",
            path: pre_xdg,
        });
    }
    markers
}

/// True when [`detect`] finds at least one legacy marker. Callers that
/// don't need the marker list use this directly.
#[must_use]
pub fn is_legacy(clone_root: &Path) -> bool {
    !detect(clone_root).is_empty()
}

/// Single-line actionable warning naming the markers found. Emitted by
/// `Store::discover` (via `store_legacy::emit_legacy_warning`) and by
/// `bl prime` for Phase 3 (bl-05e5). Empty `markers` returns an empty
/// string — callers gate on `is_legacy` before calling.
#[must_use]
pub fn warning_line(markers: &[LegacyMarker]) -> String {
    let names: Vec<String> = markers.iter().map(|m| m.path.display().to_string()).collect();
    format!(
        "warning: legacy layout in use ({}); run `bl prime --migrate` (or `bl migrate`) to relocate",
        names.join(", "),
    )
}

#[cfg(test)]
#[path = "legacy_layout_tests.rs"]
mod tests;
