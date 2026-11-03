//! Plan usage limits: how much of the subscription's 5-hour and weekly windows is used, and
//! when each resets, as the agents themselves report them.
//!
//! - Claude Code: `rate_limit_event` lines on its stream-json output. `rate_limit_info` carries
//!   the currently limiting window at the top level (`status`, `rateLimitType`, `utilization` as
//!   a fraction, `resetsAt` in unix seconds) and, from the response headers, every window in
//!   `unifiedWindows` (`five_hour`, `seven_day`, `seven_day_overage_included`, each with
//!   `utilization`, `resetsAt`, `status`). Sessions on an API key or a custom endpoint get none.
//! - Codex: `account/rateLimits/read` (asked once after `initialize`) and the sparse
//!   `account/rateLimits/updated` notification. A `RateLimitSnapshot` has `primary` and
//!   `secondary` windows with `usedPercent` (0–100), `windowDurationMins` and `resetsAt` (unix
//!   seconds).
//!
//! Both are normalized to [`PlanLimits`]. The latest per agent is kept in memory and in the
//! `meta` table (only percentages, reset times and statuses: no account ids or anything else).

use crate::claude::{Outbound, Sink};
use crate::db::{now_ms, Db};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Agents that report plan limits.
pub const PROVIDERS: &[&str] = &["claude-code", "codex"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    /// "five_hour", "weekly", "weekly_opus", "weekly_sonnet", or "window_<minutes>" for any
    /// other length (with `label`).
    pub kind: String,
    /// A short length for windows without a kind of their own ("2h", "30d").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Share of the window used, 0–100 (can pass 100 when usage runs past the cap).
    pub used_percent: f64,
    /// When the window resets, unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
    /// "allowed", "allowed_warning" or "rejected", when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanLimits {
    pub provider: String,
    pub windows: Vec<LimitWindow>,
    /// When these numbers were read from the agent, unix milliseconds.
    pub updated_at: i64,
}

static CACHE: Mutex<Option<HashMap<String, PlanLimits>>> = Mutex::new(None);

fn key(provider: &str) -> String {
    format!("plan_limits:{provider}")
}

/// Unix seconds (or milliseconds) to milliseconds.
fn to_ms(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    Some(if n > 1e12 {
        n as i64
    } else {
        (n * 1000.0) as i64
    })
}

fn status_of(v: &Value) -> Option<String> {
    v.get("status")
        .and_then(Value::as_str)
        .filter(|s| ["allowed", "allowed_warning", "rejected"].contains(s))
        .map(str::to_owned)
}

fn order(kind: &str) -> u8 {
    match kind {
        "five_hour" => 0,
        "weekly" => 1,
        "weekly_opus" => 2,
        "weekly_sonnet" => 3,
        _ => 4,
    }
}

fn sorted(mut w: Vec<LimitWindow>) -> Vec<LimitWindow> {
    w.sort_by(|a, b| {
        order(&a.kind)
            .cmp(&order(&b.kind))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    w
}

fn live(w: &LimitWindow, now: i64) -> bool {
    w.resets_at.map_or(true, |r| r > now)
}

/// `fresh` replaces windows of the same kind in `prev`; the rest of `prev` stays until it resets.
fn merge(prev: Option<&[LimitWindow]>, fresh: Vec<LimitWindow>, now: i64) -> Vec<LimitWindow> {
    let mut out: Vec<LimitWindow> = prev
        .unwrap_or_default()
        .iter()
        .filter(|w| live(w, now) && !fresh.iter().any(|f| f.kind == w.kind))
        .cloned()
        .collect();
    out.extend(fresh);
    sorted(out)
}

/* ------------------------------------------------------------ Claude Code */

fn claude_kind(t: &str) -> Option<&'static str> {
    match t {
        "five_hour" => Some("five_hour"),
        "seven_day" => Some("weekly"),
        "seven_day_opus" => Some("weekly_opus"),
        "seven_day_sonnet" => Some("weekly_sonnet"),
        // `seven_day_overage_included` is a per-model bucket that counts extra usage, and
        // `overage` is spend, not a window: neither is the plan's own limit.
        _ => None,
    }
}

/// Windows from one `rate_limit_info`, merged onto the previous ones. `now` is unix ms.
/// Empty when the event carries no usage numbers (API-key sessions, status-only events).
pub fn from_claude(info: &Value, prev: Option<&[LimitWindow]>, now: i64) -> Vec<LimitWindow> {
    let top_status = status_of(info);
    let top_kind = info
        .get("rateLimitType")
        .and_then(Value::as_str)
        .and_then(claude_kind);

    // Every window, as of this response: replaces what was known.
    if let Some(all) = info.get("unifiedWindows").and_then(Value::as_object) {
        let mut out = Vec::new();
        for (name, w) in all {
            let Some(kind) = claude_kind(name) else {
                continue;
            };
            let Some(u) = w.get("utilization").and_then(Value::as_f64) else {
                continue;
            };
            let resets_at = w
                .get("resetsAt")
                .or_else(|| w.get("resets_at"))
                .and_then(to_ms);
            let mut status = status_of(w);
            // The top level says whether requests are refused right now, and for which window.
            if top_status.as_deref() == Some("rejected") && top_kind == Some(kind) {
                status = top_status.clone();
            }
            let win = LimitWindow {
                kind: kind.into(),
                label: None,
                used_percent: (u * 100.0).max(0.0),
                resets_at,
                status,
            };
            if live(&win, now) {
                out.push(win);
            }
        }
        if !out.is_empty() {
            return sorted(out);
        }
    }

    // Older Claude Code, or no headers yet: only the currently limiting window.
    let Some(kind) = top_kind else {
        return Vec::new();
    };
    let used = match info.get("utilization").and_then(Value::as_f64) {
        Some(u) => u * 100.0,
        None if top_status.as_deref() == Some("rejected") => 100.0,
        None => return Vec::new(),
    };
    let win = LimitWindow {
        kind: kind.into(),
        label: None,
        used_percent: used.max(0.0),
        resets_at: info.get("resetsAt").and_then(to_ms),
        status: top_status,
    };
    if !live(&win, now) {
        return Vec::new();
    }
    merge(prev, vec![win], now)
}

/* ------------------------------------------------------------ Codex */

fn length_label(mins: i64) -> String {
    if mins % 1440 == 0 {
        format!("{}d", mins / 1440)
    } else if mins % 60 == 0 {
        format!("{}h", mins / 60)
    } else {
        format!("{mins}m")
    }
}

fn codex_window(w: &Value, slot: &str) -> Option<LimitWindow> {
    let used = w.get("usedPercent").and_then(Value::as_f64)?;
    let mins = w.get("windowDurationMins").and_then(Value::as_i64);
    let (kind, label) = match mins {
        Some(300) => ("five_hour".to_string(), None),
        Some(10080) => ("weekly".to_string(), None),
        Some(m) if m > 0 => (format!("window_{m}"), Some(length_label(m))),
        // No length given: Codex's primary window is the 5-hour one, the secondary the week.
        _ if slot == "primary" => ("five_hour".to_string(), None),
        _ => ("weekly".to_string(), None),
    };
    Some(LimitWindow {
        kind,
        label,
        used_percent: used.max(0.0),
        resets_at: w.get("resetsAt").and_then(to_ms),
        status: (used >= 100.0).then(|| "rejected".to_string()),
    })
}

/// Windows from a Codex `RateLimitSnapshot`. A full read (`sparse` false) replaces what was
/// known; an `account/rateLimits/updated` notification is merged onto it.
pub fn from_codex(
    snapshot: &Value,
    prev: Option<&[LimitWindow]>,
    sparse: bool,
    now: i64,
) -> Vec<LimitWindow> {
    let fresh: Vec<LimitWindow> = ["primary", "secondary"]
        .iter()
        .filter_map(|slot| codex_window(snapshot.get(*slot)?, slot))
        .filter(|w| live(w, now))
        .collect();
    if sparse {
        merge(prev, fresh, now)
    } else {
        sorted(fresh)
    }
}

/// Whether an `account/rateLimits/updated` snapshot is for the bucket the read answered with
/// (Codex can meter some models separately).
pub fn codex_same_bucket(snapshot: &Value, base: Option<&str>) -> bool {
    match snapshot.get("limitId").and_then(Value::as_str) {
        None => true,
        Some(id) => base.map_or(id == "codex", |b| b == id),
    }
}

/* ------------------------------------------------------------ store */

/// The latest limits for one agent: this run's, else the saved ones.
pub fn get(db: &Db, provider: &str) -> Option<PlanLimits> {
    if let Some(l) = CACHE
        .lock()
        .ok()
        .and_then(|c| c.as_ref().and_then(|m| m.get(provider).cloned()))
    {
        return Some(l);
    }
    let saved: PlanLimits = serde_json::from_str(&db.meta(&key(provider)).ok()??).ok()?;
    Some(saved)
}

/// Limits for every agent that has reported any.
pub fn all(db: &Db) -> Vec<PlanLimits> {
    PROVIDERS.iter().filter_map(|p| get(db, p)).collect()
}

/// The previous windows for an agent, for merging partial updates.
pub fn previous(db: &Db, provider: &str) -> Option<Vec<LimitWindow>> {
    get(db, provider).map(|l| l.windows)
}

/// Keeps and publishes new windows. Nothing happens when there are none.
pub fn record(db: &Db, sink: &Sink, provider: &str, windows: Vec<LimitWindow>) {
    if windows.is_empty() {
        return;
    }
    let limits = PlanLimits {
        provider: provider.to_string(),
        windows,
        updated_at: now_ms(),
    };
    if let Ok(mut c) = CACHE.lock() {
        c.get_or_insert_with(HashMap::new)
            .insert(provider.to_string(), limits.clone());
    }
    if let Ok(s) = serde_json::to_string(&limits) {
        let _ = db.set_meta(&key(provider), &s);
    }
    sink(Outbound::Limits(limits));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    // 2026-11-03 12:00:00 UTC.
    const NOW: i64 = 1_793_707_200_000;
    const NOW_S: i64 = NOW / 1000;

    /// A `rate_limit_event` as Claude Code 2.1 prints it on a Pro/Max login.
    fn claude_event() -> Value {
        json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "allowed_warning",
                "resetsAt": NOW_S + 3 * 3600,
                "rateLimitType": "five_hour",
                "utilization": 0.82,
                "unifiedWindows": {
                    "five_hour": { "utilization": 0.82, "resetsAt": NOW_S + 3 * 3600, "status": "allowed_warning", "surpassedThreshold": 0.8 },
                    "seven_day": { "utilization": 0.61, "resetsAt": NOW_S + 4 * 86400, "status": "allowed" },
                    "seven_day_overage_included": { "utilization": 0.4, "resetsAt": NOW_S + 4 * 86400 }
                },
                "overageStatus": "rejected",
                "overageDisabledReason": "org_level_disabled",
                "isUsingOverage": false
            },
            "uuid": "8b0c3f8e-6a55-4d0e-9a51-2f5a2b9a1c11",
            "session_id": "0a4c2f1e-1111-4222-8333-944455556666"
        })
    }

    #[test]
    fn claude_unified_windows() {
        let w = from_claude(&claude_event()["rate_limit_info"], None, NOW);
        assert_eq!(w.len(), 2, "{w:?}");
        assert_eq!(w[0].kind, "five_hour");
        assert!((w[0].used_percent - 82.0).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some((NOW_S + 3 * 3600) * 1000));
        assert_eq!(w[0].status.as_deref(), Some("allowed_warning"));
        assert_eq!(w[1].kind, "weekly");
        assert!((w[1].used_percent - 61.0).abs() < 1e-9);
        assert_eq!(w[1].status.as_deref(), Some("allowed"));
    }

    #[test]
    fn claude_rejected_and_expired() {
        let info = json!({
            "status": "rejected",
            "resetsAt": NOW_S + 600,
            "rateLimitType": "five_hour",
            "utilization": 1.0,
            "unifiedWindows": {
                "five_hour": { "utilization": 1.02, "resetsAt": NOW_S + 600 },
                // Already reset: dropped.
                "seven_day": { "utilization": 0.9, "resetsAt": NOW_S - 5 }
            },
            "isUsingOverage": false
        });
        let w = from_claude(&info, None, NOW);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].status.as_deref(), Some("rejected"));
        assert!(w[0].used_percent > 100.0);
    }

    #[test]
    fn claude_top_level_only_merges() {
        let prev = vec![LimitWindow {
            kind: "weekly".into(),
            label: None,
            used_percent: 40.0,
            resets_at: Some(NOW + 86_400_000),
            status: None,
        }];
        let info = json!({ "status": "allowed", "resetsAt": NOW_S + 7200, "rateLimitType": "five_hour", "utilization": 0.25 });
        let w = from_claude(&info, Some(&prev), NOW);
        assert_eq!(
            w.iter().map(|x| x.kind.as_str()).collect::<Vec<_>>(),
            ["five_hour", "weekly"]
        );
        assert!((w[0].used_percent - 25.0).abs() < 1e-9);
        // API-key sessions: a status with no numbers changes nothing.
        assert!(from_claude(&json!({ "status": "allowed" }), Some(&prev), NOW).is_empty());
        assert!(from_claude(
            &json!({ "status": "allowed", "rateLimitType": "overage", "utilization": 0.5 }),
            None,
            NOW
        )
        .is_empty());
    }

    /// `account/rateLimits/read` from codex app-server 0.166 on a ChatGPT Plus login.
    fn codex_read() -> Value {
        json!({
            "id": 2,
            "result": {
                "accountId": "user-AbCdEf",
                "ordinaryUsageAllowed": true,
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": null,
                    "planType": "plus",
                    "primary": { "usedPercent": 34, "windowDurationMins": 300, "resetsAt": NOW_S + 2 * 3600 },
                    "secondary": { "usedPercent": 61, "windowDurationMins": 10080, "resetsAt": NOW_S + 3 * 86400 },
                    "credits": { "hasCredits": false, "unlimited": false, "balance": null },
                    "rateLimitReachedType": null,
                    "spendControlReached": null,
                    "individualLimit": null
                },
                "rateLimitsByLimitId": { "codex": { "limitId": "codex", "primary": { "usedPercent": 34, "windowDurationMins": 300, "resetsAt": NOW_S + 2 * 3600 } } },
                "rateLimitResetCredits": { "availableCount": 0, "credits": null }
            }
        })
    }

    #[test]
    fn codex_read_snapshot() {
        let w = from_codex(&codex_read()["result"]["rateLimits"], None, false, NOW);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].kind, "five_hour");
        assert_eq!(w[0].used_percent, 34.0);
        assert_eq!(w[0].resets_at, Some((NOW_S + 7200) * 1000));
        assert_eq!(w[0].status, None);
        assert_eq!(w[1].kind, "weekly");
        assert_eq!(w[1].used_percent, 61.0);
        // Nothing from the account is kept.
        let s = serde_json::to_string(&w).unwrap();
        assert!(!s.contains("user-AbCdEf") && !s.contains("plus"));
    }

    #[test]
    fn codex_sparse_update_merges() {
        let prev = from_codex(&codex_read()["result"]["rateLimits"], None, false, NOW);
        let note = json!({
            "method": "account/rateLimits/updated",
            "params": { "rateLimits": {
                "limitId": "codex",
                "primary": { "usedPercent": 100, "windowDurationMins": 300, "resetsAt": NOW_S + 3600 },
                "secondary": null,
                "rateLimitReachedType": "rate_limit_reached"
            } }
        });
        let snap = &note["params"]["rateLimits"];
        assert!(codex_same_bucket(snap, Some("codex")));
        let w = from_codex(snap, Some(&prev), true, NOW);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].used_percent, 100.0);
        assert_eq!(w[0].status.as_deref(), Some("rejected"));
        assert_eq!(w[1].used_percent, 61.0, "secondary kept from the read");
        // Another bucket's update is not this one.
        assert!(!codex_same_bucket(
            &json!({ "limitId": "codex_other" }),
            Some("codex")
        ));
        assert!(codex_same_bucket(
            &json!({ "limitId": null }),
            Some("codex")
        ));
    }

    #[test]
    fn codex_other_lengths() {
        let snap = json!({
            "primary": { "usedPercent": 12, "windowDurationMins": 120, "resetsAt": NOW_S + 60 },
            "secondary": { "usedPercent": 5 }
        });
        let w = from_codex(&snap, None, false, NOW);
        assert_eq!(w[0].kind, "weekly", "no length: secondary is the week");
        assert_eq!(w[1].kind, "window_120");
        assert_eq!(w[1].label.as_deref(), Some("2h"));
        assert_eq!(length_label(43200), "30d");
    }

    #[test]
    fn record_saves_and_emits() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("t.db")).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let sink: Sink = Arc::new(move |o| {
            if let Outbound::Limits(l) = o {
                s2.lock().unwrap().push(l);
            }
        });
        // A provider name of its own: the cache is shared by every test.
        record(&db, &sink, "test-limits", Vec::new());
        assert!(seen.lock().unwrap().is_empty());
        let w = from_claude(&claude_event()["rate_limit_info"], None, NOW);
        record(&db, &sink, "test-limits", w.clone());
        assert_eq!(seen.lock().unwrap().len(), 1);
        let saved: PlanLimits =
            serde_json::from_str(&db.meta("plan_limits:test-limits").unwrap().unwrap()).unwrap();
        assert_eq!(saved.windows, w);
        assert!(saved.updated_at > 0);
        let v = serde_json::to_value(&saved).unwrap();
        assert!(v["windows"][0]["usedPercent"].is_number());
        assert!(v["windows"][0]["resetsAt"].is_number());
        assert!(v["updatedAt"].is_number());
        assert_eq!(get(&db, "test-limits").unwrap().windows, w);
    }

    /// A stand-in agent: a shell script, plus a session in a real folder.
    #[cfg(unix)]
    fn fake_agent(
        script: &str,
        provider: &str,
    ) -> (tempfile::TempDir, Arc<Db>, String, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("agent");
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let proj = dir.path().join("project");
        std::fs::create_dir(&proj).unwrap();
        let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
        let p = db.add_project("project", &proj.to_string_lossy()).unwrap();
        let s = db
            .create_session(&p.id, provider, "default", "New session")
            .unwrap();
        (dir, db, s.id, fake)
    }

    fn wait_limits(rx: &std::sync::mpsc::Receiver<Outbound>, n: usize) -> Vec<PlanLimits> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut got = Vec::new();
        while got.len() < n && std::time::Instant::now() < deadline {
            if let Ok(Outbound::Limits(l)) = rx.recv_timeout(std::time::Duration::from_millis(100))
            {
                got.push(l);
            }
        }
        got
    }

    fn channel_sink() -> (Sink, std::sync::mpsc::Receiver<Outbound>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |o| {
            let _ = tx.lock().unwrap().send(o);
        });
        (sink, rx)
    }

    /// Claude Code's `rate_limit_event` becomes the plan's limits, not a session event.
    #[cfg(unix)]
    #[test]
    fn claude_session_captures_rate_limit_event() {
        let r = now_ms() / 1000;
        let line = json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "allowed", "resetsAt": r + 7200, "rateLimitType": "five_hour", "utilization": 0.34,
                "unifiedWindows": {
                    "five_hour": { "utilization": 0.34, "resetsAt": r + 7200, "status": "allowed" },
                    "seven_day": { "utilization": 0.61, "resetsAt": r + 3 * 86400, "status": "allowed" }
                },
                "isUsingOverage": false
            },
            "uuid": "u-1", "session_id": "s-1"
        });
        let script = format!("#!/bin/sh\necho '{line}'\nwhile read -r l; do :; done\n");
        let (_dir, db, sid, fake) = fake_agent(&script, "claude-code");
        let (sink, rx) = channel_sink();
        let mut mgr = crate::claude::Manager::new(db.clone(), sink);
        mgr.binary_override = Some(fake);
        let mgr = Arc::new(mgr);
        mgr.send_message(&sid, "hi").unwrap();
        let got = wait_limits(&rx, 1);
        mgr.stop(&sid);
        assert_eq!(got.len(), 1, "no limits event");
        assert_eq!(got[0].provider, "claude-code");
        let kinds: Vec<_> = got[0]
            .windows
            .iter()
            .map(|w| (w.kind.as_str(), w.used_percent.round() as i64))
            .collect();
        assert_eq!(kinds, [("five_hour", 34), ("weekly", 61)]);
        assert!(db.meta("plan_limits:claude-code").unwrap().is_some());
        assert!(
            !db.events(&sid)
                .unwrap()
                .iter()
                .any(|e| e.kind.starts_with("rate_limit")),
            "not stored as a session event"
        );
    }

    /// Codex: the limits are read after `initialize`, then sparse updates are merged in.
    #[cfg(unix)]
    #[test]
    fn codex_session_reads_and_follows_limits() {
        let r = now_ms() / 1000;
        let read = json!({ "rateLimits": {
            "limitId": "codex", "limitName": null, "planType": "plus",
            "primary": { "usedPercent": 34, "windowDurationMins": 300, "resetsAt": r + 7200 },
            "secondary": { "usedPercent": 61, "windowDurationMins": 10080, "resetsAt": r + 3 * 86400 },
            "credits": null, "rateLimitReachedType": null, "spendControlReached": null, "individualLimit": null
        }, "rateLimitsByLimitId": null, "accountId": "acct-secret", "ordinaryUsageAllowed": true });
        let note = json!({ "method": "account/rateLimits/updated", "params": { "rateLimits": {
            "limitId": "codex",
            "primary": { "usedPercent": 90, "windowDurationMins": 300, "resetsAt": r + 7000 },
            "secondary": null
        } } });
        // Answers by method; ids follow the JSON-RPC requests AgentDeck sends.
        let script = format!(
            r#"#!/bin/sh
while read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/^{{"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"initialize"'*) echo "{{\"id\":$id,\"result\":{{}}}}" ;;
    *'"method":"account/rateLimits/read"'*) echo "{{\"id\":$id,\"result\":$READ}}"; echo '{note}' ;;
  esac
done
"#
        );
        let script = script.replace("$READ", &read.to_string().replace('"', "\\\""));
        let (_dir, db, sid, fake) = fake_agent(&script, "codex");
        let (sink, rx) = channel_sink();
        let mut mgr = crate::codex::CodexManager::new(db.clone(), sink);
        mgr.binary_override = Some(fake);
        let mgr = Arc::new(mgr);
        mgr.send_message(&sid, "hi").unwrap();
        let got = wait_limits(&rx, 2);
        mgr.stop(&sid);
        assert_eq!(got.len(), 2, "read and update: {got:?}");
        let pct = |l: &PlanLimits| {
            l.windows
                .iter()
                .map(|w| (w.kind.clone(), w.used_percent as i64))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pct(&got[0]),
            [("five_hour".to_string(), 34), ("weekly".to_string(), 61)]
        );
        assert_eq!(
            pct(&got[1]),
            [("five_hour".to_string(), 90), ("weekly".to_string(), 61)]
        );
        let saved = db.meta("plan_limits:codex").unwrap().unwrap();
        assert!(!saved.contains("acct-secret") && !saved.contains("plus"));
    }
}
