//! Bounded-output + wall-clock-timeout wrapper for plugin
//! subprocesses. Enforces:
//!
//! - Max stdout/stderr buffered: `BALLS_PLUGIN_MAX_STREAM_BYTES` or
//!   1 MiB by default. Reader threads keep draining past the cap so
//!   the plugin never blocks on a full pipe.
//! - Wall-clock cap on the whole invocation:
//!   `BALLS_PLUGIN_TIMEOUT_SECS` or 30s by default. On timeout, the
//!   child's entire process group is SIGKILL'd — a plugin is
//!   typically a shell that forks, and killing just the shell
//!   leaves orphaned children holding our stdout pipe open.

use crate::error::Result;
use std::fs::File;
use std::io::{Read, Write};
use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

const DEFAULT_MAX_STREAM_BYTES: usize = 1024 * 1024;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const POLL_INTERVAL: Duration = Duration::from_millis(25);

pub fn max_stream_bytes() -> usize {
    std::env::var("BALLS_PLUGIN_MAX_STREAM_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_STREAM_BYTES)
}

pub fn timeout() -> Duration {
    let secs = std::env::var("BALLS_PLUGIN_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

pub struct PluginOutcome {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Raw bytes read from the diagnostics channel (FD 3 in the child).
    /// Empty when the plugin ignored the channel or didn't have one.
    pub diagnostics: Vec<u8>,
    /// True if stdout exceeded the per-stream byte cap.
    pub truncated: bool,
    /// True if the plugin was killed because it exceeded the wall clock.
    pub timed_out: bool,
}

/// Run `child` to completion with bounded output and a wall-clock
/// timeout. Feeds `stdin_bytes` on a writer thread so the main
/// thread never blocks on a full stdin pipe if the plugin isn't
/// reading. If `diag_read` is provided, the parent also drains the
/// diagnostics channel in a separate thread; a plugin that ignores
/// the channel yields an empty `diagnostics` buffer. Returns
/// `PluginOutcome` with flags for the two failure modes.
pub fn run_with_limits(
    mut child: Child,
    stdin_bytes: &[u8],
    diag_read: Option<File>,
) -> Result<PluginOutcome> {
    if let Some(mut sin) = child.stdin.take() {
        let bytes = stdin_bytes.to_vec();
        std::thread::spawn(move || {
            let _ = sin.write_all(&bytes);
            // sin drops here, closing the pipe so the child sees EOF.
        });
    }

    let cap = max_stream_bytes();
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stdout_thread = std::thread::spawn(move || drain_capped(stdout, cap));
    let stderr_thread = std::thread::spawn(move || drain_capped(stderr, cap));
    let diag_thread = diag_read.map(|r| std::thread::spawn(move || drain_capped(r, cap)));

    let deadline = Instant::now() + timeout();
    let mut timed_out = false;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            kill_process_group(child.id());
            timed_out = true;
            break child.wait()?;
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let (stdout_buf, stdout_trunc) = stdout_thread.join().unwrap_or_default();
    let (stderr_buf, _) = stderr_thread.join().unwrap_or_default();
    let diagnostics = diag_thread
        .map(|t| t.join().unwrap_or_default().0)
        .unwrap_or_default();

    Ok(PluginOutcome {
        status,
        stdout: stdout_buf,
        stderr: stderr_buf,
        diagnostics,
        truncated: stdout_trunc,
        timed_out,
    })
}

/// Read `r` to EOF while retaining only the first `cap` bytes.
/// Continues draining past the cap so the producer never blocks on
/// pipe-full. Returns `(bytes, truncated)`. A read error is treated
/// as EOF — we return what we've got rather than propagate, since
/// the child is already on its way out by the time we see one.
fn drain_capped<R: Read>(mut r: R, cap: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::with_capacity(cap.min(64 * 1024));
    let mut truncated = false;
    let mut tmp = [0u8; 8192];
    while let Ok(n) = r.read(&mut tmp) {
        if n == 0 {
            break;
        }
        if buf.len() < cap {
            let room = cap - buf.len();
            let take = n.min(room);
            buf.extend_from_slice(&tmp[..take]);
            if take < n {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    (buf, truncated)
}

/// SIGKILL the process group led by `pid`. Relies on the child
/// having been spawned with `process_group(0)`, so pgid == pid.
/// Direct syscall: `/bin/kill -KILL -{pid}` parses ambiguously across
/// kill implementations and was observed to hang on GitHub Actions.
fn kill_process_group(child_pid: u32) {
    #[allow(clippy::cast_possible_wrap)]
    let pgid = child_pid as i32;
    // SAFETY: killpg is async-signal-safe; any error is ignored because
    // the timeout path has no recovery beyond "kill harder".
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct Chunks(Vec<io::Result<&'static [u8]>>);
    impl Read for Chunks {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                return Ok(0);
            }
            match self.0.remove(0) {
                Ok(bytes) => {
                    let n = bytes.len().min(buf.len());
                    buf[..n].copy_from_slice(&bytes[..n]);
                    Ok(n)
                }
                Err(e) => Err(e),
            }
        }
    }

    #[test]
    fn drain_capped_clean_read() {
        let r = Chunks(vec![Ok(b"hello world")]);
        let (buf, trunc) = drain_capped(r, 100);
        assert_eq!(buf, b"hello world");
        assert!(!trunc);
    }

    #[test]
    fn drain_capped_truncates_when_first_read_exceeds_cap() {
        let r = Chunks(vec![Ok(b"hello world")]);
        let (buf, trunc) = drain_capped(r, 5);
        assert_eq!(buf, b"hello");
        assert!(trunc);
    }

    #[test]
    fn drain_capped_keeps_draining_after_cap_reached() {
        // Three separate reads; cap is small enough that the first
        // fills it and the next two land in the "else" branch.
        let r = Chunks(vec![Ok(b"abcd"), Ok(b"efgh"), Ok(b"ijkl")]);
        let (buf, trunc) = drain_capped(r, 4);
        assert_eq!(buf, b"abcd");
        assert!(trunc);
    }

    #[test]
    fn drain_capped_treats_read_error_as_eof() {
        // After one successful chunk, the reader errors. Drain
        // should return what it has without panicking.
        let r = Chunks(vec![
            Ok(b"partial"),
            Err(io::Error::other("boom")),
        ]);
        let (buf, trunc) = drain_capped(r, 100);
        assert_eq!(buf, b"partial");
        assert!(!trunc);
    }
}
