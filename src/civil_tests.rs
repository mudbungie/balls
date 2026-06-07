use super::*;

#[test]
fn the_epoch_renders_as_the_start_of_1970() {
    assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
}

#[test]
fn a_known_timestamp_renders_its_civil_date() {
    // 1748357520 = 2025-05-27T14:52:00Z (the §3 example timestamp).
    assert_eq!(iso8601(1_748_357_520), "2025-05-27T14:52:00Z");
}

#[test]
fn the_time_of_day_splits_into_hms() {
    // One hour, two minutes, three seconds past the epoch.
    assert_eq!(iso8601(3723), "1970-01-01T01:02:03Z");
    // The last second of a day.
    assert_eq!(iso8601(86_399), "1970-01-01T23:59:59Z");
}

#[test]
fn a_leap_day_is_rendered() {
    // 2024-02-29T00:00:00Z = 19782 days * 86400.
    assert_eq!(iso8601(19_782 * 86_400), "2024-02-29T00:00:00Z");
}

#[test]
fn a_pre_epoch_timestamp_floors_to_its_true_civil_date() {
    // -1 second is the last second of 1969, not a negative wrap.
    assert_eq!(iso8601(-1), "1969-12-31T23:59:59Z");
    // A full day before the epoch.
    assert_eq!(iso8601(-86_400), "1969-12-31T00:00:00Z");
}

#[test]
fn a_far_future_year_renders_four_digits() {
    // 2100 is NOT a leap year (divisible by 100, not 400) — exercises the
    // century rule in civil_from_days.
    assert_eq!(iso8601(4_107_542_400), "2100-03-01T00:00:00Z");
}
