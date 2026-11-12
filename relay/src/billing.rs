//! Paying for the relay: per-seat monthly subscriptions through Lemon Squeezy.
//!
//! Billing is **off** unless the relay is started with Lemon Squeezy's settings (see
//! [`Billing::from_env`]); a self-hosted relay without them has no limits. With billing on:
//!
//! - A new team gets a free trial ([`Billing::trial_days`] days, up to [`Billing::trial_seats`]
//!   people). Its first member is its admin.
//! - An admin subscribes from AgentDeck: the relay makes a Lemon Squeezy checkout for the number
//!   of seats (the team's id goes along as custom data) and the app opens it in the browser.
//! - Lemon Squeezy tells the relay what happened through its webhook (`POST
//!   /v1/billing/lemonsqueezy`, signed with HMAC-SHA256 of the raw body in `X-Signature`): the
//!   subscription's status, seats, renewal or end date, and the customer portal's address.
//! - An admin changes the number of seats from AgentDeck (Lemon Squeezy prorates it); never
//!   below the number of members.
//! - What a team may do follows from its plan ([`access`]): while the trial or subscription is
//!   good (including a payment that's being retried, and a cancelled one until its period ends),
//!   everything works; after that the team can still read, but can't share, send or add people.
//!   Joining also needs a free seat.
//! - The relay operator can give a team a complimentary plan (`agentdeck-relay billing comp`).

use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

#[derive(Clone, Debug)]
pub struct Billing {
    pub api_key: String,
    pub store_id: String,
    pub variant_id: String,
    pub webhook_secret: String,
    pub trial_days: u32,
    pub trial_seats: i64,
    /// Where Lemon Squeezy sends people after paying (optional).
    pub redirect_url: Option<String>,
    /// Lemon Squeezy's API; tests point it at a stand-in.
    pub api_base: String,
}

impl Billing {
    /// `LEMONSQUEEZY_API_KEY`, `LEMONSQUEEZY_STORE_ID`, `LEMONSQUEEZY_VARIANT_ID` and
    /// `LEMONSQUEEZY_WEBHOOK_SECRET` turn billing on (all four, or none);
    /// `RELAY_TRIAL_DAYS` (14), `RELAY_TRIAL_SEATS` (10), `RELAY_CHECKOUT_REDIRECT` are optional.
    pub fn from_env() -> Result<Option<Self>, String> {
        let get = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let keys = [
            "LEMONSQUEEZY_API_KEY",
            "LEMONSQUEEZY_STORE_ID",
            "LEMONSQUEEZY_VARIANT_ID",
            "LEMONSQUEEZY_WEBHOOK_SECRET",
        ];
        let vals: Vec<Option<String>> = keys.iter().map(|k| get(k)).collect();
        if vals.iter().all(Option::is_none) {
            return Ok(None);
        }
        if let Some(i) = vals.iter().position(Option::is_none) {
            return Err(format!(
                "Billing needs all of {}; {} is missing",
                keys.join(", "),
                keys[i]
            ));
        }
        let v: Vec<String> = vals.into_iter().flatten().collect();
        Ok(Some(Self {
            api_key: v[0].clone(),
            store_id: v[1].clone(),
            variant_id: v[2].clone(),
            webhook_secret: v[3].clone(),
            trial_days: get("RELAY_TRIAL_DAYS")
                .and_then(|d| d.parse().ok())
                .unwrap_or(14),
            trial_seats: get("RELAY_TRIAL_SEATS")
                .and_then(|d| d.parse().ok())
                .unwrap_or(10),
            redirect_url: get("RELAY_CHECKOUT_REDIRECT"),
            api_base: "https://api.lemonsqueezy.com".into(),
        }))
    }

    fn client(&self) -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())
    }

    async fn call(&self, req: reqwest::RequestBuilder) -> Result<Value, String> {
        let r = req
            .bearer_auth(&self.api_key)
            .header("Accept", "application/vnd.api+json")
            .header("Content-Type", "application/vnd.api+json")
            .send()
            .await
            .map_err(|e| format!("Couldn't reach Lemon Squeezy: {e}"))?;
        let status = r.status();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            Ok(body)
        } else {
            eprintln!("agentdeck-relay: Lemon Squeezy answered {status}: {body}");
            Err(format!("Lemon Squeezy refused it ({status})"))
        }
    }

    /// A checkout for `seats` seats of the team, prefilled with the admin's email. Its URL.
    pub async fn checkout(&self, team_id: &str, email: &str, seats: i64) -> Result<String, String> {
        let variant: i64 = self
            .variant_id
            .parse()
            .map_err(|_| "LEMONSQUEEZY_VARIANT_ID isn't a number")?;
        let mut attributes = json!({
            "checkout_data": {
                "email": email,
                "custom": { "team_id": team_id },
                "variant_quantities": [{ "variant_id": variant, "quantity": seats }]
            }
        });
        if let Some(u) = &self.redirect_url {
            attributes["product_options"] = json!({ "redirect_url": u });
        }
        let body = json!({ "data": {
            "type": "checkouts",
            "attributes": attributes,
            "relationships": {
                "store": { "data": { "type": "stores", "id": self.store_id } },
                "variant": { "data": { "type": "variants", "id": self.variant_id } }
            }
        }});
        let v = self
            .call(
                self.client()?
                    .post(format!("{}/v1/checkouts", self.api_base))
                    .json(&body),
            )
            .await?;
        v.pointer("/data/attributes/url")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "Lemon Squeezy sent no checkout address".into())
    }

    /// Changes the subscription's quantity (seats). Lemon Squeezy prorates it.
    pub async fn set_seats(&self, item_id: &str, seats: i64) -> Result<(), String> {
        let body = json!({ "data": { "type": "subscription-items", "id": item_id, "attributes": { "quantity": seats } } });
        self.call(
            self.client()?
                .patch(format!("{}/v1/subscription-items/{item_id}", self.api_base))
                .json(&body),
        )
        .await
        .map(|_| ())
    }
}

/// Lemon Squeezy's webhook signature: the hex HMAC-SHA256 of the raw body, compared in constant time.
pub fn verify(secret: &str, body: &[u8], signature: &str) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let Some(sig) = hex(signature.trim()) else {
        return false;
    };
    mac.verify_slice(&sig).is_ok()
}

pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length works");
    mac.update(body);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// "2026-11-12T07:43:37.000000Z" (or with an offset) → ms since 1970.
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let (time, offset_min) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0)
    } else if let Some(i) = rest.rfind(['+', '-']) {
        let (t, off) = rest.split_at(i);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let mut o = off[1..].split(':').map(|x| x.parse::<i64>().ok());
        (
            t,
            sign * (o.next()?? * 60 + o.next().flatten().unwrap_or(0)),
        )
    } else {
        (rest, 0)
    };
    let mut t = time.split(':');
    let (h, mi) = (
        t.next()?.parse::<i64>().ok()?,
        t.next()?.parse::<i64>().ok()?,
    );
    let sec: f64 = t.next().unwrap_or("0").parse().ok()?;
    // Days from civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(
        ((days * 86_400 + h * 3600 + mi * 60 - offset_min * 60) as f64 * 1000.0 + sec * 1000.0)
            as i64,
    )
}

/// A team's plan as the relay keeps it.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// "trialing" (the relay's own trial), "comp" (given by the operator), or Lemon Squeezy's:
    /// "on_trial", "active", "past_due", "unpaid", "paused", "cancelled", "expired".
    pub status: String,
    pub seats: Option<i64>,
    pub trial_ends_at: Option<i64>,
    pub renews_at: Option<i64>,
    pub ends_at: Option<i64>,
}

/// What a team may do now.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    pub can_write: bool,
    /// None: no limit.
    pub seats: Option<i64>,
    /// Why writing is off, or a warning while it still works (a failed payment, a trial ending).
    pub note: Option<String>,
}

const DAY: i64 = 86_400_000;

pub fn access(billing_on: bool, p: &Plan, now: i64) -> Access {
    if !billing_on || p.status == "comp" {
        return Access {
            can_write: true,
            seats: if billing_on { p.seats } else { None },
            note: None,
        };
    }
    let ok = |note: Option<String>| Access {
        can_write: true,
        seats: p.seats,
        note,
    };
    let off = |why: &str| Access {
        can_write: false,
        seats: p.seats,
        note: Some(why.into()),
    };
    match p.status.as_str() {
        "trialing" | "on_trial" => match p.trial_ends_at {
            Some(end) if end <= now => off("The free trial has ended. A team admin can subscribe in AgentDeck: Team view → connection → Plan."),
            Some(end) if end - now < 3 * DAY => ok(Some(format!("The free trial ends in {} day(s).", ((end - now) / DAY).max(1)))),
            _ => ok(None),
        },
        "active" => ok(None),
        "past_due" => ok(Some("The last payment didn't go through; it's being retried. A team admin can update the card in the billing portal.".into())),
        "cancelled" => match p.ends_at {
            Some(end) if end > now => ok(Some("The subscription is cancelled and ends at the end of this period.".into())),
            _ => off("The subscription has ended. A team admin can subscribe again in AgentDeck."),
        },
        "paused" => off("The subscription is paused. A team admin can resume it in the billing portal."),
        "unpaid" => off("Payments failed and the subscription is on hold. A team admin can update the card in the billing portal."),
        "expired" => off("The subscription has expired. A team admin can subscribe again in AgentDeck."),
        _ => off("This team has no plan yet. A team admin can subscribe in AgentDeck."),
    }
}

/// What a subscription webhook says, for the team it belongs to.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubscriptionUpdate {
    pub event: String,
    /// From the checkout's custom data (subscription_created), else matched by subscription id.
    pub team_id: Option<String>,
    pub subscription_id: String,
    pub item_id: Option<String>,
    pub plan: Plan,
    pub portal_url: Option<String>,
    pub payment_url: Option<String>,
    pub email: Option<String>,
}

/// Reads a Lemon Squeezy webhook body; None for events other than subscriptions'.
pub fn parse_webhook(v: &Value) -> Option<SubscriptionUpdate> {
    let event = v.pointer("/meta/event_name")?.as_str()?.to_string();
    if !event.starts_with("subscription_") || v.pointer("/data/type")?.as_str()? != "subscriptions"
    {
        return None;
    }
    let a = v.pointer("/data/attributes")?;
    let s = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_owned);
    let t = |k: &str| a.get(k).and_then(Value::as_str).and_then(parse_time);
    let id = match v.pointer("/data/id")? {
        Value::String(s) => s.clone(),
        n => n.to_string(),
    };
    let item = a.get("first_subscription_item");
    Some(SubscriptionUpdate {
        event,
        team_id: v
            .pointer("/meta/custom_data/team_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        subscription_id: id,
        item_id: item.and_then(|i| i.get("id")).map(|i| match i {
            Value::String(s) => s.clone(),
            n => n.to_string(),
        }),
        plan: Plan {
            status: s("status").unwrap_or_default(),
            seats: item.and_then(|i| i.get("quantity")).and_then(Value::as_i64),
            trial_ends_at: t("trial_ends_at"),
            renews_at: t("renews_at"),
            ends_at: t("ends_at"),
        },
        portal_url: a
            .pointer("/urls/customer_portal")
            .and_then(Value::as_str)
            .map(str::to_owned),
        payment_url: a
            .pointer("/urls/update_payment_method")
            .and_then(Value::as_str)
            .map(str::to_owned),
        email: s("user_email"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures() {
        let body = br#"{"meta":{"event_name":"subscription_created"}}"#;
        let sig = sign("s3cret", body);
        assert_eq!(sig.len(), 64);
        assert!(verify("s3cret", body, &sig));
        assert!(verify("s3cret", body, &sig.to_uppercase()));
        assert!(!verify("other", body, &sig));
        assert!(!verify("s3cret", b"{}", &sig));
        assert!(!verify("s3cret", body, "zz"));
        assert!(!verify("s3cret", body, ""));
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_time("2026-11-12T07:43:37.000000Z"),
            Some(1_794_469_417_000)
        );
        assert_eq!(
            parse_time("2026-11-12T10:43:37+03:00"),
            Some(1_794_469_417_000)
        );
        assert_eq!(
            parse_time("2000-02-29T12:00:00.500Z"),
            Some(951_825_600_500)
        );
        assert_eq!(parse_time("nope"), None);
    }

    #[test]
    fn what_a_plan_allows() {
        let now = 1_000 * DAY;
        let p = |status: &str| Plan {
            status: status.into(),
            seats: Some(5),
            ..Default::default()
        };
        assert!(
            access(false, &p("expired"), now).can_write,
            "no billing, no limits"
        );
        assert_eq!(access(false, &p("expired"), now).seats, None);
        assert!(access(true, &p("active"), now).can_write);
        assert!(access(true, &p("comp"), now).can_write);
        let past_due = access(true, &p("past_due"), now);
        assert!(past_due.can_write && past_due.note.is_some());
        for s in ["unpaid", "paused", "expired", "", "weird"] {
            assert!(!access(true, &p(s), now).can_write, "{s}");
        }
        let trial = |end: i64| Plan {
            status: "trialing".into(),
            seats: Some(10),
            trial_ends_at: Some(end),
            ..Default::default()
        };
        assert!(access(true, &trial(now + 10 * DAY), now).note.is_none());
        assert!(access(true, &trial(now + DAY), now)
            .note
            .unwrap()
            .contains("ends in 1 day"));
        assert!(!access(true, &trial(now - 1), now).can_write);
        let cancelled = |end: i64| Plan {
            status: "cancelled".into(),
            ends_at: Some(end),
            ..Default::default()
        };
        assert!(access(true, &cancelled(now + DAY), now).can_write);
        assert!(!access(true, &cancelled(now - DAY), now).can_write);
    }

    #[test]
    fn reads_a_subscription_webhook() {
        let v = json!({
            "meta": { "event_name": "subscription_created", "custom_data": { "team_id": "t_1" } },
            "data": { "type": "subscriptions", "id": "123", "attributes": {
                "status": "active", "user_email": "ayse@acme.dev",
                "renews_at": "2026-12-12T07:43:37.000000Z", "ends_at": null, "trial_ends_at": null,
                "first_subscription_item": { "id": 456, "subscription_id": 123, "quantity": 7 },
                "urls": { "update_payment_method": "https://pay", "customer_portal": "https://portal" }
            }}
        });
        let u = parse_webhook(&v).unwrap();
        assert_eq!(u.team_id.as_deref(), Some("t_1"));
        assert_eq!(
            (u.subscription_id.as_str(), u.item_id.as_deref()),
            ("123", Some("456"))
        );
        assert_eq!((u.plan.status.as_str(), u.plan.seats), ("active", Some(7)));
        assert!(u.plan.renews_at.is_some() && u.plan.ends_at.is_none());
        assert_eq!(u.portal_url.as_deref(), Some("https://portal"));
        let mut order = v.clone();
        order["meta"]["event_name"] = "order_created".into();
        assert!(parse_webhook(&order).is_none());
    }
}
