//! Task ID and priority validation. Split out of `task.rs` for the
//! same reason `task_io.rs` is — that file stays focused on type
//! definitions while behavior lives in its own module — and to keep
//! both under the 300-line cap. Re-exported from `task` so call sites
//! keep importing `crate::task::{validate_id, ...}` unchanged.

use crate::error::{BallError, Result};

/// Inclusive range for task priority. `1` is highest urgency, `4`
/// lowest. Centralized here so a future widening (e.g. to `1..=5`)
/// touches exactly one place.
pub const PRIORITY_MIN: u8 = 1;
pub const PRIORITY_MAX: u8 = 4;

/// Reject priorities outside `PRIORITY_MIN..=PRIORITY_MAX`. Used by
/// the `bl create` path, which already receives a `u8` from clap.
pub fn validate_priority(p: u8) -> Result<()> {
    if !(PRIORITY_MIN..=PRIORITY_MAX).contains(&p) {
        return Err(BallError::InvalidTask(format!(
            "priority must be {PRIORITY_MIN}..={PRIORITY_MAX}"
        )));
    }
    Ok(())
}

/// Parse a priority from a user-supplied string (e.g. the `value`
/// half of `bl update priority=3`). Rejects non-integers and
/// out-of-range values with a consistent error message.
pub fn parse_priority(s: &str) -> Result<u8> {
    let p: u8 = s
        .parse()
        .map_err(|_| BallError::InvalidTask(format!("priority not integer: {s}")))?;
    validate_priority(p)?;
    Ok(p)
}

/// Validate that a task ID is safe for use in file paths.
///
/// IDs must match `bl-[0-9a-fA-F]+`. `generate_id` only ever emits
/// lowercase hex, but the loader accepts uppercase or mixed case so
/// a future `bl` that changes its generator does not break older
/// clients reading the same repo. The `bl-` prefix itself is
/// deliberately strict: mixing `bl-` and `BL-` in one repo would
/// fragment task filenames, so we surface that as a hard error.
pub fn validate_id(id: &str) -> Result<()> {
    let valid = id.starts_with("bl-")
        && id.len() > 3
        && id[3..].chars().all(|c| c.is_ascii_hexdigit());
    if !valid {
        return Err(BallError::InvalidTask(format!("invalid task id: {id}")));
    }
    Ok(())
}
