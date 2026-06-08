//! Unix-time → ISO-8601 rendering — the SOLE place storage's i64 seconds (§3)
//! become a human date (§9). Storage and transit are ALWAYS unix-time; only this
//! display layer converts, so the rest of balls needs no date library and no
//! `chrono` dependency. A hand-rolled civil-from-days (Howard Hinnant's
//! algorithm) is ~25 lines and exact for any i64, including pre-1970 negatives.

/// Render unix `seconds` as ISO-8601 UTC, e.g. `2025-05-27T15:32:00Z`. Pure;
/// total over all of `i64` (`div_euclid`/`rem_euclid` floor toward negative
/// infinity, so a pre-epoch timestamp renders its true civil date).
#[must_use]
pub fn iso8601(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Parse a `YYYY-MM-DD` calendar date to the unix second at its `00:00:00` UTC
/// start — the one inverse of [`iso8601`], for the `bl list` date-window filters
/// (§9). `None` on any malformed field or out-of-range month/day; the day is the
/// only place a human date flows BACK to storage's i64, so it lives here beside
/// its forward twin. Total for every well-formed date via Hinnant's
/// `days_from_civil` (the exact inverse of [`civil_from_days`]).
#[must_use]
pub fn start_of_day(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// The count of days since the unix epoch for a civil `(y, m, d)` — Hinnant's
/// branch-free inverse of [`civil_from_days`], same March-based internal year.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // March-based month [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // day-of-year [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // day-of-era [0, 146096]
    era * 146_097 + doe - 719_468
}

/// The civil `(year, month, day)` for a count of days since the unix epoch
/// (1970-01-01 = day 0). Hinnant's branch-free algorithm, shifted to a
/// March-based internal year so the leap day lands last. All-i64 so the small
/// month/day results need no narrowing cast (they format identically).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // day-of-era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day-of-year [0, 365]
    let mp = (5 * doy + 2) / 153; // March-based month [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
#[path = "civil_tests.rs"]
mod tests;
