//! Billing over real HTTP, with a stand-in for Lemon Squeezy's API.

use agentdeck_relay::billing::{sign, Billing, Plan};
use agentdeck_relay::{router, Relay, Store};
use axum::{
    extract::Path,
    routing::{patch, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const SECRET: &str = "whsec_test";

/// Lemon Squeezy's API as far as the relay uses it; remembers what it was sent.
async fn fake_lemonsqueezy() -> (String, Arc<Mutex<Vec<(String, Value, String)>>>) {
    let seen: Arc<Mutex<Vec<(String, Value, String)>>> = Arc::default();
    let (s1, s2) = (seen.clone(), seen.clone());
    let app = Router::new()
        .route(
            "/v1/checkouts",
            post(move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                let s = s1.clone();
                async move {
                    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                    s.lock().unwrap().push(("checkout".into(), body, auth));
                    Json(json!({ "data": { "type": "checkouts", "id": "c1", "attributes": { "url": "https://acme.lemonsqueezy.com/checkout/custom/abc" } } }))
                }
            }),
        )
        .route(
            "/v1/subscription-items/{id}",
            patch(move |Path(id): Path<String>, Json(body): Json<Value>| {
                let s = s2.clone();
                async move {
                    s.lock().unwrap().push((format!("patch {id}"), body, String::new()));
                    Json(json!({ "data": { "type": "subscription-items", "id": id } }))
                }
            }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (base, seen)
}

async fn relay(open_signup: bool, ls: &str) -> (String, Arc<Relay>) {
    let billing = Billing {
        api_key: "ls_key".into(),
        store_id: "11".into(),
        variant_id: "22".into(),
        webhook_secret: SECRET.into(),
        trial_days: 14,
        trial_seats: 2,
        redirect_url: None,
        api_base: ls.into(),
    };
    let relay = Relay::with(Store::open_in_memory().unwrap(), Some(billing), open_signup);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let app = router(relay.clone());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (base, relay)
}

struct C {
    base: String,
    http: reqwest::Client,
}

impl C {
    async fn post(&self, token: Option<&str>, path: &str, body: Value) -> (u16, Value) {
        let mut r = self.http.post(format!("{}{path}", self.base)).json(&body);
        if let Some(t) = token {
            r = r.bearer_auth(t);
        }
        let r = r.send().await.unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
    async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
    async fn webhook(&self, body: &Value, sig: Option<String>) -> u16 {
        let raw = serde_json::to_vec(body).unwrap();
        let sig = sig.unwrap_or_else(|| sign(SECRET, &raw));
        self.http
            .post(format!("{}/v1/billing/lemonsqueezy", self.base))
            .header("X-Signature", sig)
            .header("content-type", "application/json")
            .body(raw)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }
}

fn subscription(event: &str, team: &str, status: &str, seats: i64) -> Value {
    json!({
        "meta": { "event_name": event, "custom_data": { "team_id": team } },
        "data": { "type": "subscriptions", "id": "9001", "attributes": {
            "status": status, "user_email": "ayse@acme.dev",
            "renews_at": "2030-01-01T00:00:00.000000Z", "ends_at": null, "trial_ends_at": null,
            "first_subscription_item": { "id": 456, "subscription_id": 9001, "quantity": seats },
            "urls": { "update_payment_method": "https://pay.example", "customer_portal": "https://portal.example" }
        }}
    })
}

fn note(id: &str) -> Value {
    json!({ "id": id, "thread": id, "to": "*", "kind": "note", "text": "hi" })
}

#[tokio::test]
async fn a_team_signs_up_tries_subscribes_grows_and_lapses() {
    let (ls, seen) = fake_lemonsqueezy().await;
    let (base, relay) = relay(true, &ls).await;
    let c = C {
        base,
        http: reqwest::Client::new(),
    };

    // Ayşe creates the team: she's its admin, it's on the trial (2 seats here).
    let (st, made) = c
        .post(
            None,
            "/v1/teams",
            json!({ "team": "Acme", "name": "Ayşe", "email": "ayse@acme.dev" }),
        )
        .await;
    assert_eq!(st, 200, "{made}");
    let ta = made["token"].as_str().unwrap().to_string();
    let team = made["team"]["id"].as_str().unwrap().to_string();
    let invite = made["invite"].as_str().unwrap().to_string();
    assert_eq!(made["member"]["admin"], true);
    let b = c.get(&ta, "/v1/billing").await.1;
    assert_eq!(
        (
            b["enabled"].clone(),
            b["plan"]["status"].clone(),
            b["plan"]["seats"].clone(),
            b["used"].clone()
        ),
        (json!(true), json!("trialing"), json!(2), json!(1))
    );
    assert_eq!(b["access"]["canWrite"], true);

    // Mert joins and takes the second seat; Zeynep finds them all taken.
    let (st, m) = c
        .post(
            None,
            "/v1/join",
            json!({ "invite": invite, "name": "Mert", "email": "mert@acme.dev" }),
        )
        .await;
    assert_eq!(st, 200);
    assert_eq!(m["member"]["admin"], false);
    let tm = m["token"].as_str().unwrap().to_string();
    let (st, full) = c
        .post(
            None,
            "/v1/join",
            json!({ "invite": invite, "name": "Zeynep", "email": "zeynep@acme.dev" }),
        )
        .await;
    assert_eq!(st, 402);
    assert!(full["error"].as_str().unwrap().contains("All 2 seats"));
    // Mert again on another Mac doesn't need a seat.
    assert_eq!(
        c.post(
            None,
            "/v1/join",
            json!({ "invite": invite, "name": "Mert", "email": "mert@acme.dev" })
        )
        .await
        .0,
        200
    );

    // Only an admin subscribes, for at least as many seats as there are people.
    assert_eq!(
        c.post(Some(&tm), "/v1/billing/checkout", json!({ "seats": 5 }))
            .await
            .0,
        403
    );
    assert_eq!(
        c.post(Some(&ta), "/v1/billing/checkout", json!({ "seats": 1 }))
            .await
            .0,
        400
    );
    let (st, co) = c
        .post(Some(&ta), "/v1/billing/checkout", json!({ "seats": 5 }))
        .await;
    assert_eq!(st, 200, "{co}");
    assert_eq!(
        co["url"],
        "https://acme.lemonsqueezy.com/checkout/custom/abc"
    );
    {
        let s = seen.lock().unwrap();
        let (kind, body, auth) = &s[0];
        assert_eq!(
            (kind.as_str(), auth.as_str()),
            ("checkout", "Bearer ls_key")
        );
        let cd = &body["data"]["attributes"]["checkout_data"];
        assert_eq!(cd["custom"]["team_id"], team.as_str());
        assert_eq!(cd["email"], "ayse@acme.dev");
        assert_eq!(
            cd["variant_quantities"],
            json!([{ "variant_id": 22, "quantity": 5 }])
        );
        assert_eq!(body["data"]["relationships"]["store"]["data"]["id"], "11");
        assert_eq!(body["data"]["relationships"]["variant"]["data"]["id"], "22");
    }

    // Lemon Squeezy confirms: only a correctly signed webhook counts.
    let created = subscription("subscription_created", &team, "active", 5);
    assert_eq!(c.webhook(&created, Some("00".repeat(32))).await, 401);
    assert_eq!(c.webhook(&created, None).await, 200);
    let b = c.get(&ta, "/v1/billing").await.1;
    assert_eq!(
        (
            b["plan"]["status"].clone(),
            b["plan"]["seats"].clone(),
            b["subscribed"].clone()
        ),
        (json!("active"), json!(5), json!(true))
    );
    assert_eq!(b["portalUrl"], "https://portal.example");
    assert_eq!(
        c.get(&tm, "/v1/billing").await.1["portalUrl"],
        Value::Null,
        "only admins see the portal"
    );
    assert_eq!(
        c.post(
            None,
            "/v1/join",
            json!({ "invite": invite, "name": "Zeynep", "email": "zeynep@acme.dev" })
        )
        .await
        .0,
        200
    );

    // Seats change through Lemon Squeezy, never below the team.
    assert_eq!(
        c.post(Some(&ta), "/v1/billing/seats", json!({ "seats": 2 }))
            .await
            .0,
        400
    );
    assert_eq!(
        c.post(Some(&ta), "/v1/billing/seats", json!({ "seats": 6 }))
            .await
            .0,
        200
    );
    {
        let s = seen.lock().unwrap();
        let (kind, body, _) = s.last().unwrap();
        assert_eq!(kind, "patch 456");
        assert_eq!(body["data"]["attributes"]["quantity"], 6);
    }
    assert_eq!(c.get(&ta, "/v1/billing").await.1["plan"]["seats"], 6);

    // A failed payment warns but works; an expired subscription stops writing, not reading.
    let repo = "/v1/messages?repo=github.com/acme/shop";
    assert_eq!(
        c.webhook(
            &subscription("subscription_payment_failed", &team, "past_due", 6),
            None
        )
        .await,
        200
    );
    assert_eq!(c.post(Some(&tm), repo, note("n1")).await.0, 200);
    assert!(c.get(&tm, "/v1/billing").await.1["access"]["note"]
        .as_str()
        .unwrap()
        .contains("payment"));
    assert_eq!(
        c.webhook(
            &subscription("subscription_expired", &team, "expired", 6),
            None
        )
        .await,
        200
    );
    let (st, err) = c.post(Some(&tm), repo, note("n2")).await;
    assert_eq!(st, 402);
    assert!(err["error"].as_str().unwrap().contains("expired"));
    assert_eq!(c.get(&tm, repo).await.0, 200);
    let (st, _) = c
        .post(
            None,
            "/v1/join",
            json!({ "invite": invite, "name": "Deniz", "email": "deniz@acme.dev" }),
        )
        .await;
    assert_eq!(st, 402);

    // The operator can give a team a complimentary plan.
    relay
        .store
        .set_plan(
            &team,
            &Plan {
                status: "comp".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(c.post(Some(&tm), repo, note("n3")).await.0, 200);

    // A webhook for a subscription that matches no team is acknowledged and ignored.
    let mut stray = subscription("subscription_created", "t_nope", "active", 1);
    stray["data"]["id"] = "777".into();
    assert_eq!(c.webhook(&stray, None).await, 200);
}

#[tokio::test]
async fn trials_end_and_signups_can_be_closed() {
    let (ls, _) = fake_lemonsqueezy().await;
    let (base, relay) = relay(false, &ls).await;
    let c = C {
        base,
        http: reqwest::Client::new(),
    };
    let (st, _) = c
        .post(
            None,
            "/v1/teams",
            json!({ "team": "X", "name": "A", "email": "a@x.dev" }),
        )
        .await;
    assert_eq!(st, 403, "by invite only");

    let team = relay.create_team("Acme").unwrap();
    let invite = relay.store.create_invite(&team.id, None, "").unwrap();
    let ta = c
        .post(
            None,
            "/v1/join",
            json!({ "invite": invite, "name": "Ayşe", "email": "ayse@acme.dev" }),
        )
        .await
        .1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let mut plan = relay.store.plan(&team.id).unwrap();
    assert_eq!(plan.status, "trialing");
    plan.trial_ends_at = Some(1);
    relay.store.set_plan(&team.id, &plan).unwrap();
    let (st, err) = c.post(Some(&ta), "/v1/messages?repo=r", note("n1")).await;
    assert_eq!(st, 402);
    assert!(err["error"].as_str().unwrap().contains("trial has ended"));
    // The admin can still subscribe.
    assert_eq!(
        c.post(Some(&ta), "/v1/billing/checkout", json!({ "seats": 3 }))
            .await
            .0,
        200
    );
}
