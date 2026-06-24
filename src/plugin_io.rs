//! §6 subprocess IO plumbing — the exec/stdin/stderr mechanics the dispatch
//! ([`super::Subprocess`]) and the `protocol` self-describe share: the transient
//! `ETXTBSY` exec retry, stdin payload delivery, and the bounded stderr relay.
//! Lifted from [`super`] so the dispatch file stays the wire/recursion logic.

use std::io::{self, BufRead, Read, Write};

/// Bounded retries for a transient `ETXTBSY` when exec'ing a plugin binary —
/// see [`retry_busy`].
const BUSY_RETRIES: u32 = 6;
const BUSY_BACKOFF_MS: u64 = 2;

/// Retry `exec` while it reports `ExecutableFileBusy`. Exec'ing a file any
/// process holds open for writing yields `ETXTBSY` — transient when a plugin
/// binary is being (re)written concurrently (a parallel agent's `bl install`,
/// §6). Bounded, then gives up with the last error. Shared with the §6 read-op
/// dispatch ([`crate::reads`]), which spawns the same plugin binaries.
pub(crate) fn retry_busy<T>(mut exec: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    for _ in 0..BUSY_RETRIES {
        match exec() {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(BUSY_BACKOFF_MS));
            }
            other => return other,
        }
    }
    exec()
}

/// Deliver `payload` to a plugin's stdin (consuming the pipe → EOF). A plugin may
/// reject the op by exiting BEFORE it drains stdin (§7: the exit STATUS, not delivery,
/// is authoritative), closing the pipe — swallow that one `BrokenPipe` so `child.wait`
/// reports the plugin's real nonzero exit, not a masking "broken pipe" (bl-0100); any other write error propagates.
pub(super) fn feed(mut stdin: impl Write, payload: &str) -> io::Result<()> {
    match stdin.write_all(payload.as_bytes()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e),
        _ => Ok(()),
    }
}

/// A very generous per-line ceiling on enveloped plugin stderr (1 MiB): far above
/// any real diagnostic, but bounded, so a no-newline flood cannot OOM the parent.
pub(super) const RELAY_LINE_MAX: u64 = 1 << 20;

/// Hand `reader`'s lines to `sink`, never buffering more than `cap` bytes at once:
/// a line longer than `cap` is flushed in `cap`-sized pieces rather than grown
/// without bound. The trailing newline is trimmed; a read error ends the relay.
pub(super) fn capped_lines(mut reader: impl BufRead, cap: u64, mut sink: impl FnMut(&str)) {
    let mut buf = Vec::new();
    while reader.by_ref().take(cap).read_until(b'\n', &mut buf).unwrap_or(0) != 0 {
        if buf.last() == Some(&b'\n') {
            buf.pop();
        }
        sink(&String::from_utf8_lossy(&buf));
        buf.clear();
    }
}
