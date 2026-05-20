//! Shared fixtures for the `render_show` / `render_show_relations`
//! unit tests. Declared at crate level (cfg(test)) so both themed
//! test modules — which live under different parent modules after the
//! relations split — reach the same builders without duplication.

use crate::delivery::Delivery;
use crate::display::Display;
use crate::render_show::Ctx;
use crate::task::{NewTaskOpts, Status, Task};
use chrono::{Duration, TimeZone, Utc};

pub(crate) fn now_fixed() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap()
}

pub(crate) fn ctx_for<'a>() -> Ctx<'a> {
    Ctx {
        d: Display::plain(),
        me: "me",
        columns: 80,
        verbose: false,
        now: now_fixed(),
    }
}

pub(crate) fn empty_delivery() -> Delivery {
    Delivery { sha: None, hint_stale: false }
}

pub(crate) fn mk(id: &str, title: &str) -> Task {
    let mut t = Task::new(
        NewTaskOpts {
            title: title.into(),
            ..Default::default()
        },
        id.into(),
    );
    t.status = Status::Open;
    let when = now_fixed() - Duration::hours(2);
    t.created_at = when;
    t.updated_at = when;
    t
}
