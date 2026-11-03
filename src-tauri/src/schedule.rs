//! When a bot's schedule runs next.
//!
//! Daily schedules run at "HH:MM" times in local time on chosen weekdays (none chosen: every
//! day); interval schedules run every N minutes (at least 15). Times are worked out in a time
//! zone (`chrono::Local` in the app, a fixed rule in tests), so daylight-saving changes neither
//! run a slot twice nor skip a day: a time that happens twice (clocks go back) runs at its first
//! occurrence, and a time that doesn't exist (clocks go forward) runs an hour later that day.
//! Runs that were missed while AgentDeck or the Mac was off aren't made up: a due time more
//! than `GRACE_MS` in the past is skipped and the next run is worked out from now.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};

pub const MIN_EVERY: i64 = 15;
pub const MAX_EVERY: i64 = 7 * 24 * 60;
pub const MAX_TIMES: usize = 24;
/// How late a due run may still start (the scheduler wakes every 30 seconds).
pub const GRACE_MS: i64 = 120_000;

/// What decides when a schedule runs.
#[derive(Debug, Clone, PartialEq)]
pub enum When {
    /// (hour, minute) pairs, sorted; weekdays 1 (Monday) to 7 (Sunday), empty: every day.
    Daily {
        times: Vec<(u32, u32)>,
        weekdays: Vec<u8>,
    },
    Every(i64),
}

/// "9:05" or "09:05" → (9, 5).
pub fn parse_time(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.trim().split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// A schedule's times, weekdays and minutes as they're stored.
pub type Checked = (Vec<String>, Vec<u8>, Option<i64>);

/// Checks what the editor sends and returns it in a stored shape: times as sorted "HH:MM"
/// without repeats, weekdays sorted without repeats (all seven: empty), minutes as given.
pub fn check(
    kind: &str,
    times: &[String],
    weekdays: &[u8],
    every_minutes: Option<i64>,
) -> Result<Checked, String> {
    match kind {
        "daily" => {
            let mut parsed = Vec::new();
            for t in times {
                parsed.push(parse_time(t).ok_or_else(|| format!("Not a time of day: {t}"))?);
            }
            parsed.sort_unstable();
            parsed.dedup();
            if parsed.is_empty() {
                return Err("Pick at least one time of day".into());
            }
            if parsed.len() > MAX_TIMES {
                return Err(format!("Up to {MAX_TIMES} times a day"));
            }
            let mut days = weekdays.to_vec();
            if days.iter().any(|d| !(1..=7).contains(d)) {
                return Err("Weekdays go from 1 (Monday) to 7 (Sunday)".into());
            }
            days.sort_unstable();
            days.dedup();
            if days.len() == 7 {
                days.clear();
            }
            Ok((
                parsed
                    .into_iter()
                    .map(|(h, m)| format!("{h:02}:{m:02}"))
                    .collect(),
                days,
                None,
            ))
        }
        "interval" => {
            let every = every_minutes.ok_or("Say how often it runs")?;
            if !(MIN_EVERY..=MAX_EVERY).contains(&every) {
                return Err(format!(
                    "Run it every {MIN_EVERY} minutes at most, and at least once a week"
                ));
            }
            Ok((Vec::new(), Vec::new(), Some(every)))
        }
        other => Err(format!("Unknown kind of schedule: {other}")),
    }
}

/// The schedule's rule from its stored fields (None: it can't run, e.g. no times).
pub fn when(kind: &str, times: &[String], weekdays: &[u8], every: Option<i64>) -> Option<When> {
    match kind {
        "daily" => {
            let mut times: Vec<(u32, u32)> = times.iter().filter_map(|t| parse_time(t)).collect();
            times.sort_unstable();
            (!times.is_empty()).then(|| When::Daily {
                times,
                weekdays: weekdays.to_vec(),
            })
        }
        "interval" => every.filter(|e| *e >= MIN_EVERY).map(When::Every),
        _ => None,
    }
}

/// The instant a local wall-clock time stands for: its first occurrence when clocks go back,
/// an hour later when it falls in the gap of clocks going forward.
fn resolve<Tz: TimeZone>(tz: &Tz, local: NaiveDateTime) -> Option<DateTime<Utc>> {
    let t = match tz.from_local_datetime(&local) {
        LocalResult::Single(t) => t,
        LocalResult::Ambiguous(first, _) => first,
        LocalResult::None => tz
            .from_local_datetime(&(local + Duration::hours(1)))
            .earliest()?,
    };
    Some(t.with_timezone(&Utc))
}

/// The first run strictly after `after` (ms since 1970).
pub fn next_run<Tz: TimeZone>(w: &When, after: i64, tz: &Tz) -> Option<i64> {
    match w {
        When::Every(m) => Some(after + m * 60_000),
        When::Daily { times, weekdays } => {
            let after_utc = Utc.timestamp_millis_opt(after).single()?;
            let today = after_utc.with_timezone(tz).date_naive();
            // From the day before (a time pushed past midnight by a gap), a week and a bit on.
            for d in -1..=8 {
                let date = today.checked_add_signed(Duration::days(d))?;
                let wd = date.weekday().number_from_monday() as u8;
                if !weekdays.is_empty() && !weekdays.contains(&wd) {
                    continue;
                }
                let best = times
                    .iter()
                    .filter_map(|&(h, m)| resolve(tz, date.and_hms_opt(h, m, 0)?))
                    .filter(|t| *t > after_utc)
                    .min();
                if let Some(t) = best {
                    return Some(t.timestamp_millis());
                }
            }
            None
        }
    }
}

/// The next run after one that was due at `due` and ran at `now`. An interval keeps its
/// rhythm (due + every) unless that's already past; a daily schedule takes its next time.
pub fn next_after_run<Tz: TimeZone>(w: &When, due: i64, now: i64, tz: &Tz) -> Option<i64> {
    match w {
        When::Every(m) => {
            let n = due + m * 60_000;
            Some(if n > now { n } else { now + m * 60_000 })
        }
        When::Daily { .. } => next_run(w, now.max(due), tz),
    }
}

/// Whether a due run is too late to start (it was missed while AgentDeck or the Mac was off).
pub fn missed(due: i64, now: i64) -> bool {
    now - due > GRACE_MS
}

#[cfg(test)]
pub mod testing {
    use chrono::{FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone};

    /// Central European time in 2026: UTC+1, and UTC+2 from 29 March 01:00 UTC to 25 October
    /// 01:00 UTC. Fixed so tests don't depend on the machine's time zone.
    #[derive(Debug, Clone, Copy)]
    pub struct Cet2026;

    const STD: i32 = 3600;
    const DST: i32 = 7200;

    fn dst_at(utc: &NaiveDateTime) -> bool {
        let start = NaiveDate::from_ymd_opt(2026, 3, 29)
            .unwrap()
            .and_hms_opt(1, 0, 0)
            .unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 10, 25)
            .unwrap()
            .and_hms_opt(1, 0, 0)
            .unwrap();
        *utc >= start && *utc < end
    }

    impl TimeZone for Cet2026 {
        type Offset = FixedOffset;
        fn from_offset(_: &FixedOffset) -> Self {
            Cet2026
        }
        fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<FixedOffset> {
            self.offset_from_local_datetime(&local.and_hms_opt(12, 0, 0).unwrap())
        }
        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<FixedOffset> {
            // Earlier instant first (the larger offset), as chrono's own zones do.
            let fits: Vec<i32> = [DST, STD]
                .into_iter()
                .filter(|&off| {
                    let utc = *local - chrono::Duration::seconds(off as i64);
                    (if dst_at(&utc) { DST } else { STD }) == off
                })
                .collect();
            let fo = |s: i32| FixedOffset::east_opt(s).unwrap();
            match fits.as_slice() {
                [a] => LocalResult::Single(fo(*a)),
                [a, b] => LocalResult::Ambiguous(fo(*a), fo(*b)),
                _ => LocalResult::None,
            }
        }
        fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
            self.offset_from_utc_datetime(&utc.and_hms_opt(12, 0, 0).unwrap())
        }
        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
            FixedOffset::east_opt(if dst_at(utc) { DST } else { STD }).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Cet2026;
    use super::*;
    use chrono::NaiveDate;

    /// Local wall-clock time in the test zone, as ms.
    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        resolve(
            &Cet2026,
            NaiveDate::from_ymd_opt(y, mo, d)
                .unwrap()
                .and_hms_opt(h, mi, 0)
                .unwrap(),
        )
        .unwrap()
        .timestamp_millis()
    }

    fn show(ms: i64) -> String {
        Utc.timestamp_millis_opt(ms)
            .unwrap()
            .with_timezone(&Cet2026)
            .format("%a %Y-%m-%d %H:%M %:z")
            .to_string()
    }

    fn daily(times: &[&str], weekdays: &[u8]) -> When {
        let (t, w, _) = check(
            "daily",
            &times.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            weekdays,
            None,
        )
        .unwrap();
        when("daily", &t, &w, None).unwrap()
    }

    #[test]
    fn checks_what_the_editor_sends() {
        assert_eq!(parse_time("9:05"), Some((9, 5)));
        assert_eq!(parse_time("23:59"), Some((23, 59)));
        for bad in ["24:00", "9:5", "9", "a:bc", "12:60", ":30", "123:00"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
        let (t, w, e) = check(
            "daily",
            &["17:30".into(), "9:00".into(), "09:00".into()],
            &[5, 1, 1],
            None,
        )
        .unwrap();
        assert_eq!(
            (t, w, e),
            (vec!["09:00".into(), "17:30".into()], vec![1, 5], None)
        );
        let (_, w, _) = check("daily", &["9:00".into()], &[1, 2, 3, 4, 5, 6, 7], None).unwrap();
        assert!(w.is_empty(), "every day is stored as no days");
        assert!(check("daily", &[], &[], None).is_err());
        assert!(check("daily", &["9:00".into()], &[0], None).is_err());
        assert!(check("interval", &[], &[], Some(14)).is_err());
        assert!(check("interval", &[], &[], None).is_err());
        assert_eq!(
            check("interval", &["9:00".into()], &[1], Some(15)).unwrap(),
            (vec![], vec![], Some(15))
        );
        assert!(check("hourly", &[], &[], None).is_err());
    }

    #[test]
    fn daily_runs_at_the_next_time_on_a_chosen_weekday() {
        // Tuesday 3 November 2026, 08:00.
        let w = daily(&["09:00", "17:30"], &[1, 2, 3, 4, 5]);
        let tue = local(2026, 11, 3, 8, 0);
        assert_eq!(
            show(next_run(&w, tue, &Cet2026).unwrap()),
            "Tue 2026-11-03 09:00 +01:00"
        );
        let n = local(2026, 11, 3, 9, 0);
        assert_eq!(
            show(next_run(&w, n, &Cet2026).unwrap()),
            "Tue 2026-11-03 17:30 +01:00",
            "strictly after: the run that just happened isn't next"
        );
        // Friday evening: next is Monday morning.
        let fri = local(2026, 11, 6, 18, 0);
        assert_eq!(
            show(next_run(&w, fri, &Cet2026).unwrap()),
            "Mon 2026-11-09 09:00 +01:00"
        );
        // Weekends only.
        let we = daily(&["10:00"], &[6, 7]);
        assert_eq!(
            show(next_run(&we, tue, &Cet2026).unwrap()),
            "Sat 2026-11-07 10:00 +01:00"
        );
        // Every day.
        let all = daily(&["07:15"], &[]);
        assert_eq!(
            show(next_run(&all, tue, &Cet2026).unwrap()),
            "Wed 2026-11-04 07:15 +01:00"
        );
    }

    #[test]
    fn intervals_keep_their_rhythm() {
        let w = When::Every(90);
        assert_eq!(next_run(&w, 0, &Cet2026), Some(90 * 60_000));
        // Ran 20 s late: the next stays on the rhythm.
        assert_eq!(
            next_after_run(&w, 1_000_000, 1_020_000, &Cet2026),
            Some(1_000_000 + 5_400_000)
        );
        // Far behind (it was missed): counted from now.
        assert_eq!(
            next_after_run(&w, 0, 10_000_000, &Cet2026),
            Some(10_000_000 + 5_400_000)
        );
    }

    #[test]
    fn daylight_saving_changes_neither_double_run_nor_skip_a_day() {
        // Clocks go back on Sunday 25 October 2026: 02:30 happens twice.
        let w = daily(&["02:30"], &[]);
        let sat = local(2026, 10, 24, 12, 0);
        let first = next_run(&w, sat, &Cet2026).unwrap();
        assert_eq!(
            show(first),
            "Sun 2026-10-25 02:30 +02:00",
            "the first 02:30"
        );
        // It runs, and the next run is Monday, not the second 02:30 an hour later.
        let next = next_after_run(&w, first, first + 20_000, &Cet2026).unwrap();
        assert_eq!(show(next), "Mon 2026-10-26 02:30 +01:00");
        // Computed during the repeated hour (after the first run), still Monday.
        assert_eq!(
            next_run(&w, first + 3_600_000 + 60_000, &Cet2026),
            Some(next)
        );

        // Clocks go forward on Sunday 29 March 2026: 02:30 doesn't exist; it runs at 03:30.
        let sat = local(2026, 3, 28, 12, 0);
        let gap = next_run(&w, sat, &Cet2026).unwrap();
        assert_eq!(show(gap), "Sun 2026-03-29 03:30 +02:00", "not skipped");
        let after = next_after_run(&w, gap, gap + 20_000, &Cet2026).unwrap();
        assert_eq!(show(after), "Mon 2026-03-30 02:30 +02:00");
        // Two times around the gap both run, in order.
        let two = daily(&["02:30", "03:00"], &[]);
        let a = next_run(&two, sat + 12 * 3_600_000, &Cet2026).unwrap();
        assert_eq!(show(a), "Sun 2026-03-29 03:00 +02:00");
        let b = next_after_run(&two, a, a + 20_000, &Cet2026).unwrap();
        assert_eq!(show(b), "Sun 2026-03-29 03:30 +02:00");
        // A daily 09:00 across the change: one run each day, one day apart on the clock.
        let nine = daily(&["09:00"], &[]);
        let mut t = local(2026, 3, 27, 10, 0);
        let mut seen = Vec::new();
        for _ in 0..4 {
            t = next_run(&nine, t, &Cet2026).unwrap();
            seen.push(show(t));
        }
        assert_eq!(
            seen,
            [
                "Sat 2026-03-28 09:00 +01:00",
                "Sun 2026-03-29 09:00 +02:00",
                "Mon 2026-03-30 09:00 +02:00",
                "Tue 2026-03-31 09:00 +02:00"
            ]
        );
    }

    #[test]
    fn missed_runs_are_not_made_up() {
        let due = local(2026, 11, 3, 9, 0);
        assert!(!missed(due, due + 30_000), "a tick late is fine");
        assert!(missed(due, due + 3 * 3_600_000), "the Mac was asleep");
        // The next run is counted from now: one run, at the next time.
        let w = daily(&["09:00"], &[]);
        let now = local(2026, 11, 5, 10, 0);
        assert_eq!(
            show(next_run(&w, now, &Cet2026).unwrap()),
            "Fri 2026-11-06 09:00 +01:00"
        );
    }
}
