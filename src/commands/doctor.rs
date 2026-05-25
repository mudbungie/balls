//! `bl doctor` — print repo/bl drift, suggest the fix, change nothing.

use balls::doctor::{diagnose, Finding};
use balls::error::Result;
use std::env;
use std::fmt::Write;

pub fn cmd_doctor() -> Result<()> {
    let findings = diagnose(&env::current_dir()?);
    print!("{}", render(&findings));
    Ok(())
}

/// Render the findings to the string `cmd_doctor` would print. Pulled
/// out as a pure function so the empty-findings branch (currently
/// unreachable from integration tests — pre-Phase-1B `bl init` always
/// produces a legacy clone, so the legacy-layout finding fires every
/// time) has direct unit coverage without depending on an XDG-clean
/// fixture.
#[must_use]
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::new();
    if findings.is_empty() {
        out.push_str("doctor: no problems detected.\n");
        return out;
    }
    let _ = writeln!(out, "doctor: {} problem(s) detected:", findings.len());
    for (i, f) in findings.iter().enumerate() {
        let _ = writeln!(out, "\n{}. {}", i + 1, f.problem);
        if let Some(h) = &f.hint {
            let _ = writeln!(out, "   -> {h}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::render;
    use balls::doctor::Finding;

    #[test]
    fn empty_findings_says_no_problems() {
        assert_eq!(render(&[]), "doctor: no problems detected.\n");
    }

    #[test]
    fn one_finding_with_hint_renders_numbered_block() {
        let f = Finding::flag("legacy layout", "run `bl prime --migrate`");
        let s = render(std::slice::from_ref(&f));
        assert!(s.contains("1 problem(s) detected"));
        assert!(s.contains("1. legacy layout"));
        assert!(s.contains("-> run `bl prime --migrate`"));
    }

    #[test]
    fn finding_without_hint_omits_arrow_line() {
        let f = Finding { problem: "uninitialized".into(), hint: None };
        let s = render(std::slice::from_ref(&f));
        assert!(s.contains("1. uninitialized"));
        assert!(!s.contains("->"));
    }
}
