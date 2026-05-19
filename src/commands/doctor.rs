//! `bl doctor` — print repo/bl drift, suggest the fix, change nothing.

use balls::doctor::{diagnose, Finding};
use balls::error::Result;
use std::env;

pub fn cmd_doctor() -> Result<()> {
    let findings = diagnose(&env::current_dir()?);
    if findings.is_empty() {
        println!("doctor: no problems detected.");
        return Ok(());
    }
    println!("doctor: {} problem(s) detected:", findings.len());
    for (i, f) in findings.iter().enumerate() {
        print_finding(i + 1, f);
    }
    Ok(())
}

fn print_finding(n: usize, f: &Finding) {
    println!("\n{n}. {}", f.problem);
    if let Some(h) = &f.hint {
        println!("   -> {h}");
    }
}
