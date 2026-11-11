//! Time tracking: how long agents worked, how long they waited for you, and how long you spent
//! with them, per project, session, branch and day.
//!
//! - **Agent time** and **waiting time** are measured: every change of a session's status opens
//!   or closes a stretch in `session_time` (see `Db::track_status`), with the branch it was on.
//! - **Your time** is estimated from when you wrote to, approved for or answered an agent:
//!   the time since your previous one counts when it's under [`FOCUS_GAP_MS`] (you were working
//!   with the agents), otherwise [`TOUCH_MS`] (reading the reply and writing one). You're one
//!   person, so this runs over all your sessions together and is never counted twice. Bots'
//!   sessions don't count: nobody types there.
//!
//! Days are your local days: the interface passes its UTC offset.

use crate::db::TimeSpan;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const DAY_MS: i64 = 86_400_000;
/// Up to this long between two of your messages counts as time spent with the agents.
pub const FOCUS_GAP_MS: i64 = 15 * 60_000;
/// What one message, approval or answer counts for after a longer break.
pub const TOUCH_MS: i64 = 2 * 60_000;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimeRow {
    pub project_id: String,
    pub session_id: String,
    pub branch: Option<String>,
    /// Local day, "2026-11-11".
    pub day: String,
    pub agent_ms: i64,
    pub waiting_ms: i64,
    pub you_ms: i64,
}

/// One branch's time on one day, as shared with the team in your presence.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BranchDay {
    pub branch: String,
    pub day: String,
    #[serde(default)]
    pub agent_ms: i64,
    #[serde(default)]
    pub waiting_ms: i64,
    #[serde(default)]
    pub you_ms: i64,
}

/// Days since 1970-01-01 → "YYYY-MM-DD" (proleptic Gregorian, Howard Hinnant's algorithm).
pub fn day_string(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The local day number of a moment, for a UTC offset in minutes (+180 for Istanbul).
fn local_day(ms: i64, tz_min: i64) -> i64 {
    (ms + tz_min * 60_000).div_euclid(DAY_MS)
}

/// Splits [start, end) at local midnights: (day, ms) pieces.
fn by_day(start: i64, end: i64, tz_min: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut s = start;
    while s < end {
        let day = local_day(s, tz_min);
        let next = (day + 1) * DAY_MS - tz_min * 60_000;
        let e = end.min(next);
        out.push((day, e - s));
        s = e;
    }
    out
}

type Key = (String, String, Option<String>, i64);

/// Everything tracked since `since`, by project, session, branch and local day.
/// `humans`: the sessions you work in yourself (your time is only counted there).
/// `branch_now`: a session's branch for your time outside any tracked stretch.
pub fn report(
    spans: &[TimeSpan],
    interactions: &[(String, i64)],
    projects: &HashMap<String, String>,
    humans: &HashSet<String>,
    branch_now: &HashMap<String, Option<String>>,
    since: i64,
    tz_min: i64,
) -> Vec<TimeRow> {
    let mut acc: BTreeMap<Key, (i64, i64, i64)> = BTreeMap::new();
    for s in spans {
        let start = s.start.max(since);
        if s.end <= start {
            continue;
        }
        for (day, ms) in by_day(start, s.end, tz_min) {
            let e = acc
                .entry((
                    s.project_id.clone(),
                    s.session_id.clone(),
                    s.branch.clone(),
                    day,
                ))
                .or_default();
            if s.status == "waiting" {
                e.1 += ms;
            } else {
                e.0 += ms;
            }
        }
    }
    // Your time: one timeline over every session you work in.
    let mut prev: Option<i64> = None;
    let mut mine: Vec<&(String, i64)> = interactions
        .iter()
        .filter(|(id, _)| humans.contains(id))
        .collect();
    mine.sort_by_key(|(_, ts)| *ts);
    for (sid, ts) in mine {
        let credit = match prev {
            Some(p) if ts - p <= FOCUS_GAP_MS => ts - p,
            _ => TOUCH_MS,
        };
        prev = Some(*ts);
        if *ts < since {
            continue;
        }
        let Some(project) = projects.get(sid) else {
            continue;
        };
        // The branch the session was on then: the stretch around it, else the last one before.
        let branch = spans
            .iter()
            .filter(|s| &s.session_id == sid && s.start <= *ts)
            .max_by_key(|s| s.start)
            .map(|s| s.branch.clone())
            .unwrap_or_else(|| branch_now.get(sid).cloned().flatten());
        let e = acc
            .entry((project.clone(), sid.clone(), branch, local_day(*ts, tz_min)))
            .or_default();
        e.2 += credit;
    }
    acc.into_iter()
        .map(
            |((project_id, session_id, branch, day), (a, w, y))| TimeRow {
                project_id,
                session_id,
                branch,
                day: day_string(day),
                agent_ms: a,
                waiting_ms: w,
                you_ms: y,
            },
        )
        .collect()
}

/// A project's rows summed per branch and day, newest days first, for sharing with the team.
pub fn branch_days(rows: &[TimeRow], project_id: &str, limit: usize) -> Vec<BranchDay> {
    let mut acc: BTreeMap<(String, String), BranchDay> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.project_id == project_id) {
        let Some(b) = &r.branch else { continue };
        let e = acc
            .entry((r.day.clone(), b.clone()))
            .or_insert_with(|| BranchDay {
                branch: b.clone(),
                day: r.day.clone(),
                ..Default::default()
            });
        e.agent_ms += r.agent_ms;
        e.waiting_ms += r.waiting_ms;
        e.you_ms += r.you_ms;
    }
    let mut v: Vec<BranchDay> = acc.into_values().collect();
    v.sort_by(|a, b| b.day.cmp(&a.day).then(a.branch.cmp(&b.branch)));
    v.truncate(limit);
    v
}

/// A shared day row from the remote, cleaned: a real date, at most a day per measure.
pub fn clean_branch_day(d: BranchDay) -> Option<BranchDay> {
    let ok_day = d.day.len() == 10
        && d.day.chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !ok_day || d.branch.trim().is_empty() {
        return None;
    }
    let clamp = |v: i64| v.clamp(0, DAY_MS);
    Some(BranchDay {
        branch: d
            .branch
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect(),
        day: d.day,
        agent_ms: clamp(d.agent_ms),
        waiting_ms: clamp(d.waiting_ms),
        you_ms: clamp(d.you_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    const M: i64 = 60_000;
    /// 2026-11-11 00:00 UTC.
    const DAY0: i64 = 20_768 * DAY_MS;

    fn span(sid: &str, status: &str, branch: &str, start: i64, end: i64) -> TimeSpan {
        TimeSpan {
            session_id: sid.into(),
            project_id: "p1".into(),
            status: status.into(),
            branch: Some(branch.into()),
            start,
            end,
        }
    }

    fn maps() -> (
        HashMap<String, String>,
        HashSet<String>,
        HashMap<String, Option<String>>,
    ) {
        let projects = HashMap::from([
            ("a".to_string(), "p1".to_string()),
            ("b".to_string(), "p1".to_string()),
            ("bot".to_string(), "p1".to_string()),
        ]);
        let humans = HashSet::from(["a".to_string(), "b".to_string()]);
        let now = HashMap::from([
            ("a".to_string(), Some("main".to_string())),
            ("b".to_string(), Some("fix".to_string())),
        ]);
        (projects, humans, now)
    }

    #[test]
    fn dates() {
        assert_eq!(day_string(0), "1970-01-01");
        assert_eq!(day_string(20_768), "2026-11-11");
        assert_eq!(day_string(11_016), "2000-02-29");
        assert_eq!(day_string(-1), "1969-12-31");
        // 23:30 UTC on the 11th is the 12th in Istanbul (UTC+3), still the 11th in New York.
        let t = DAY0 + 23 * H + 30 * M;
        assert_eq!(day_string(local_day(t, 180)), "2026-11-12");
        assert_eq!(day_string(local_day(t, -300)), "2026-11-11");
    }

    #[test]
    fn agent_and_waiting_time_split_at_local_midnight() {
        let (p, h, n) = maps();
        // Works 23:00–01:00 UTC; waits 01:00–01:20.
        let spans = [
            span("a", "running", "feature/x", DAY0 + 23 * H, DAY0 + 25 * H),
            span(
                "a",
                "waiting",
                "feature/x",
                DAY0 + 25 * H,
                DAY0 + 25 * H + 20 * M,
            ),
        ];
        let rows = report(&spans, &[], &p, &h, &n, 0, 0);
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.day.as_str(), r.agent_ms, r.waiting_ms))
            .collect();
        assert_eq!(got, [("2026-11-11", H, 0), ("2026-11-12", H, 20 * M)]);
        // In Istanbul all of it is on the 12th.
        let rows = report(&spans, &[], &p, &h, &n, 0, 180);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].day.as_str(), rows[0].agent_ms),
            ("2026-11-12", 2 * H)
        );
        // Only what's after `since` counts.
        let rows = report(&spans, &[], &p, &h, &n, DAY0 + 24 * H + 30 * M, 0);
        assert_eq!((rows[0].agent_ms, rows[0].waiting_ms), (30 * M, 20 * M));
    }

    #[test]
    fn your_time_is_one_timeline_across_sessions_and_skips_bots() {
        let (p, h, n) = maps();
        let spans = [span("b", "running", "fix", DAY0 + 10 * H, DAY0 + 11 * H)];
        let t0 = DAY0 + 9 * H;
        let inter = vec![
            ("a".to_string(), t0),                     // after a break: 2 min
            ("b".to_string(), t0 + 5 * M),             // 5 min later, other session: 5 min
            ("bot".to_string(), t0 + 6 * M),           // a bot's task: not you
            ("a".to_string(), t0 + 10 * M),            // 5 min after your last: 5 min
            ("b".to_string(), DAY0 + 10 * H + 30 * M), // long break: 2 min, on the stretch's branch
        ];
        let rows = report(&spans, &inter, &p, &h, &n, 0, 0);
        let you = |sid: &str, b: &str| {
            rows.iter()
                .filter(|r| r.session_id == sid && r.branch.as_deref() == Some(b))
                .map(|r| r.you_ms)
                .sum::<i64>()
        };
        assert_eq!(you("a", "main"), 2 * M + 5 * M);
        assert_eq!(you("b", "fix"), 5 * M + 2 * M);
        assert!(!rows.iter().any(|r| r.session_id == "bot"));
    }

    #[test]
    fn shared_rows_are_summed_and_cleaned() {
        let (p, h, n) = maps();
        let spans = [
            span("a", "running", "x", DAY0, DAY0 + H),
            span("b", "running", "x", DAY0 + 2 * H, DAY0 + 3 * H),
            span("b", "running", "y", DAY0 + DAY_MS, DAY0 + DAY_MS + H),
        ];
        let rows = report(&spans, &[], &p, &h, &n, 0, 0);
        let days = branch_days(&rows, "p1", 10);
        assert_eq!(days.len(), 2);
        assert_eq!(
            (
                days[0].day.as_str(),
                days[0].branch.as_str(),
                days[0].agent_ms
            ),
            ("2026-11-12", "y", H)
        );
        assert_eq!((days[1].branch.as_str(), days[1].agent_ms), ("x", 2 * H));
        assert_eq!(branch_days(&rows, "p1", 1).len(), 1);

        let bad = BranchDay {
            branch: "x".into(),
            day: "2026-1-11".into(),
            ..Default::default()
        };
        assert!(clean_branch_day(bad).is_none());
        let big = BranchDay {
            branch: "x\u{7}".into(),
            day: "2026-11-11".into(),
            agent_ms: -5,
            waiting_ms: 9 * DAY_MS,
            you_ms: 1,
        };
        let c = clean_branch_day(big).unwrap();
        assert_eq!(
            (c.branch.as_str(), c.agent_ms, c.waiting_ms, c.you_ms),
            ("x", 0, DAY_MS, 1)
        );
    }
}
