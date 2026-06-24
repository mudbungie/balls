//! [`feed`] — payload delivery to a plugin's stdin. A plugin that exits before it
//! drains stdin surfaces `BrokenPipe`; §7 makes the exit STATUS authoritative, not
//! delivery, so that one error is swallowed (`wait` reports the real failure) while
//! any other write fault propagates (bl-0100).

use super::*;
use std::io::Write;

/// A `Write` that fails every write with a fixed error kind — stands in for a
/// plugin's stdin pipe in each delivery outcome without spawning a process.
struct FailWriter(io::ErrorKind);

impl Write for FailWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(self.0))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn delivers_the_payload_to_a_reader() {
    let mut sink = Vec::new();
    feed(&mut sink, "wire").unwrap();
    assert_eq!(sink, b"wire");
}

#[test]
fn swallows_broken_pipe_from_a_plugin_that_exited_before_reading() {
    // The bl-0100 case: the plugin closed stdin under our write. Its exit status,
    // collected by `child.wait`, is the real signal — delivery failing is not.
    feed(FailWriter(io::ErrorKind::BrokenPipe), "wire").unwrap();
}

#[test]
fn propagates_any_other_write_fault() {
    let mut w = FailWriter(io::ErrorKind::Other);
    assert!(w.flush().is_ok()); // the mock flushes cleanly — only writes fault
    let err = feed(w, "wire").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Other);
}
