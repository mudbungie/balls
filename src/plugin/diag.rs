//! Diagnostics channel plumbing: a pipe whose write end is dup2'd
//! into the child as FD 3 (advertised via `BALLS_DIAG_FD`) so plugins
//! can emit structured NDJSON diagnostic records out-of-band from the
//! stdout JSON protocol. Silent no-op for plugins that never inspect
//! the env var — the pipe stays empty and render_diagnostics yields
//! nothing. Runner-side parsing and rendering live in `runner.rs`.

use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::Command;

const DIAG_FD: RawFd = 3;

/// A prepared diagnostics channel: a parent-owned read end to drain
/// after the child exits, and a parent-owned write end that must be
/// dropped after spawn so the child becomes the only remaining writer
/// and the pipe EOFs naturally on exit.
pub struct DiagPipe {
    pub read: File,
    pub write: OwnedFd,
}

/// Create a pipe, arrange for the child to see the write end as FD 3,
/// and advertise the fd through `BALLS_DIAG_FD` on the command. The
/// caller spawns, drops `DiagPipe::write`, then passes `DiagPipe::read`
/// into `run_with_limits`.
pub fn prepare_diag_pipe(cmd: &mut Command) -> std::io::Result<DiagPipe> {
    let (read, write) = make_pipe()?;
    let write_raw = write.as_raw_fd();
    // SAFETY: pre_exec runs between fork and exec; dup_fd calls
    // async-signal-safe dup2 and only touches the child's fd table.
    // dup2 clears CLOEXEC on the destination, so FD 3 survives exec
    // even though the pipe endpoints (O_CLOEXEC) do not.
    unsafe {
        cmd.pre_exec(move || dup_fd(write_raw, DIAG_FD));
    }
    cmd.env("BALLS_DIAG_FD", DIAG_FD.to_string());
    Ok(DiagPipe { read, write })
}

/// Create a pipe with CLOEXEC set on both endpoints. Uses the atomic
/// `pipe2(O_CLOEXEC)` Linux syscall where available; falls back to
/// `pipe` + `fcntl(FD_CLOEXEC)` on macOS/BSD which lack pipe2. Balls
/// only calls this from the synchronous plugin-invocation path, so
/// the classical race window between pipe() and fcntl() against a
/// concurrent fork+exec on another thread is not reachable here.
fn make_pipe() -> std::io::Result<(File, OwnedFd)> {
    #[cfg(target_os = "linux")]
    let (fd_r, fd_w) = pipe2_cloexec(libc::O_CLOEXEC)?;
    #[cfg(not(target_os = "linux"))]
    let (fd_r, fd_w) = {
        let (r, w) = pipe_raw()?;
        set_cloexec(r)?;
        set_cloexec(w)?;
        (r, w)
    };
    // SAFETY: fd_r and fd_w are fresh, valid, owned fds returned by
    // the OS; ownership transfers into File/OwnedFd here.
    Ok(unsafe { (File::from_raw_fd(fd_r), OwnedFd::from_raw_fd(fd_w)) })
}

/// Linux pipe2 wrapper. Flags are passed through so tests can drive
/// the error branch with a deliberately invalid flag.
#[cfg(target_os = "linux")]
fn pipe2_cloexec(flags: libc::c_int) -> std::io::Result<(RawFd, RawFd)> {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: pipe2 writes two valid fds into `fds` on success; on
    // failure it returns non-zero without touching the array.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), flags) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((fds[0], fds[1]))
}

/// Portable pipe(2) for platforms without pipe2 (macOS, BSD). The
/// caller must follow up with `set_cloexec` on both returned fds.
#[cfg(not(target_os = "linux"))]
fn pipe_raw() -> std::io::Result<(RawFd, RawFd)> {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: pipe writes two valid fds into `fds` on success.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((fds[0], fds[1]))
}

/// Set `FD_CLOEXEC` on `fd` via fcntl. Used by the non-Linux pipe
/// path; Linux gets CLOEXEC atomically from pipe2 and skips this.
#[cfg(not(target_os = "linux"))]
fn set_cloexec(fd: RawFd) -> std::io::Result<()> {
    // SAFETY: fcntl with F_SETFD only modifies fd flags.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Unit-testable dup2 wrapper. Extracted out of the pre_exec closure
/// so the branching lines are visible to coverage tooling (tarpaulin
/// cannot instrument code that runs post-fork in the child).
fn dup_fd(from: RawFd, to: RawFd) -> std::io::Result<()> {
    // SAFETY: dup2 is async-signal-safe and only touches the fd table.
    if unsafe { libc::dup2(from, to) } == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn make_pipe_success_returns_usable_endpoints() {
        let (mut r, w) = make_pipe().expect("real pipe");
        let w_raw = w.as_raw_fd();
        // SAFETY: take ownership of the raw fd via File, forget `w` so
        // it doesn't double-close when dropped.
        let mut w_file = unsafe { File::from_raw_fd(w_raw) };
        std::mem::forget(w);
        w_file.write_all(b"ping").unwrap();
        drop(w_file);
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"ping");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pipe2_cloexec_error_branch_triggers_on_bad_flags() {
        // -1 as flags is never valid; pipe2 returns EINVAL. The macOS
        // fallback path has no equivalent injection point (pipe takes
        // no flags), but tarpaulin doesn't run on macOS either, so
        // this Linux-only test is sufficient to cover the branch in
        // the measured build.
        let err = pipe2_cloexec(-1).expect_err("bad flags must error");
        assert!(err.raw_os_error().is_some());
    }

    #[test]
    fn dup_fd_success_duplicates_descriptor() {
        const TARGET: RawFd = 900;
        dup_fd(0, TARGET).expect("dup2 to unused fd");
        // SAFETY: TARGET is a descriptor we just created via dup2.
        unsafe {
            libc::close(TARGET);
        }
    }

    #[test]
    fn dup_fd_error_branch_triggers_on_bad_source() {
        // -1 is never a valid source fd; dup2 returns EBADF.
        let err = dup_fd(-1, 901).expect_err("bad source fd must error");
        assert!(err.raw_os_error().is_some());
    }
}
