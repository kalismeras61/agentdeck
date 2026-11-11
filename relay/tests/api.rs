//! The relay over real HTTP: joining, presence, messages, who sees what, long polling.

use agentdeck_relay::{router, Relay, Store};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

struct T {
    base: String,
    relay: std::sync::Arc<Relay>,
    http: reqwest::Client,
}

async fn start() -> T {
    let relay = Relay::new(Store::open_in_memory().unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = router(relay.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    T {
        base,
        relay,
        http: reqwest::Client::new(),
    }
}

impl T {
    async fn join(&self, invite: &str, name: &str, email: &str) -> (reqwest::StatusCode, Value) {
        let r = self
            .http
            .post(format!("{}/v1/join", self.base))
            .json(&json!({ "invite": invite, "name": name, "email": email }))
            .send()
            .await
            .unwrap();
        (r.status(), r.json().await.unwrap())
    }
    async fn get(&self, token: &str, path: &str) -> (reqwest::StatusCode, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (r.status(), r.json().await.unwrap_or(Value::Null))
    }
    async fn send(
        &self,
        method: reqwest::Method,
        token: &str,
        path: &str,
        body: Value,
    ) -> (reqwest::StatusCode, Value) {
        let r = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status(), r.json().await.unwrap_or(Value::Null))
    }
}

fn msg(id: &str, to: &str, text: &str) -> Value {
    json!({ "id": id, "thread": id, "to": to, "kind": "question", "text": text, "branch": "feature/x", "agent": "Claude Code" })
}

#[tokio::test]
async fn a_team_joins_shares_presence_and_messages_only_who_should_see_them() {
    let t = start().await;
    let team = t.relay.store.create_team("Acme").unwrap();
    let invite = t.relay.store.create_invite(&team.id, None, "").unwrap();
    let other = t.relay.store.create_team("Other").unwrap();
    let other_invite = t.relay.store.create_invite(&other.id, None, "").unwrap();

    let (st, _) = t.join("adi_wrong", "Ayşe", "ayse@acme.dev").await;
    assert_eq!(st, 403);
    let (st, a) = t.join(&invite, "Ayşe", "Ayse@Acme.dev").await;
    assert_eq!(st, 200);
    let ta = a["token"].as_str().unwrap().to_string();
    assert_eq!(a["member"]["email"], "ayse@acme.dev");
    let tm = t.join(&invite, "Mert", "mert@acme.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let tz = t.join(&invite, "Zeynep", "zeynep@acme.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let tx = t.join(&other_invite, "Eve", "eve@other.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(t.get("adr_nope", "/v1/me").await.0, 401);
    assert_eq!(t.get(&ta, "/v1/me").await.1["team"]["name"], "Acme");
    let names: Vec<String> = t.get(&ta, "/v1/members").await.1["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["Ayşe", "Mert", "Zeynep"]);

    // Presence: the server signs it with the member's own name and email.
    let repo = "?repo=github.com/acme/shop";
    let (st, _) = t.send(reqwest::Method::PUT, &ta, &format!("/v1/presence{repo}"), json!({ "name": "Mallory", "email": "ceo@acme.dev", "branch": "feature/x", "sessions": [] })).await;
    assert_eq!(st, 200);
    let p = t.get(&tm, &format!("/v1/presence{repo}")).await.1;
    assert_eq!(p["presence"][0]["email"], "ayse@acme.dev");
    assert_eq!(p["presence"][0]["name"], "Ayşe");
    assert_eq!(p["presence"][0]["branch"], "feature/x");
    // Leaving takes it off.
    t.send(
        reqwest::Method::PUT,
        &tz,
        &format!("/v1/presence{repo}"),
        json!({}),
    )
    .await;
    assert_eq!(
        t.get(&tm, &format!("/v1/presence{repo}")).await.1["presence"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        t.send(
            reqwest::Method::DELETE,
            &tz,
            &format!("/v1/presence{repo}"),
            json!({})
        )
        .await
        .0,
        200
    );
    assert_eq!(
        t.get(&tm, &format!("/v1/presence{repo}")).await.1["presence"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Another team sees none of it.
    assert_eq!(
        t.get(&tx, &format!("/v1/presence{repo}")).await.1["presence"],
        json!([])
    );

    // Messages: from is the sender's, whatever they claim; only sender, recipient and "*" readers see it.
    let mut forged = msg("q1", "mert@acme.dev", "Is login on /v2 now?");
    forged["from"] = json!({ "name": "Boss", "email": "boss@acme.dev" });
    let (st, sent) = t
        .send(
            reqwest::Method::POST,
            &ta,
            &format!("/v1/messages{repo}"),
            forged,
        )
        .await;
    assert_eq!(st, 200, "{sent}");
    assert_eq!(sent["from"]["email"], "ayse@acme.dev");
    t.send(
        reqwest::Method::POST,
        &ta,
        &format!("/v1/messages{repo}"),
        json!({ "id": "n1", "thread": "n1", "to": "*", "kind": "note", "text": "Deploying at 5" }),
    )
    .await;
    let ids = |v: Value| {
        v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(t.get(&tm, &format!("/v1/messages{repo}")).await.1),
        ["q1", "n1"]
    );
    assert_eq!(
        ids(t.get(&tz, &format!("/v1/messages{repo}")).await.1),
        ["n1"]
    );
    assert_eq!(
        ids(t.get(&ta, &format!("/v1/messages{repo}")).await.1),
        ["q1", "n1"]
    );
    assert_eq!(
        ids(t.get(&tx, &format!("/v1/messages{repo}")).await.1),
        Vec::<String>::new()
    );
    // Another repository is another conversation.
    assert_eq!(
        ids(t.get(&tm, "/v1/messages?repo=github.com/acme/api").await.1),
        Vec::<String>::new()
    );

    // Bad messages are refused.
    for (body, why) in [
        (msg("q1", "mert@acme.dev", "again"), "already sent"),
        (msg("bad id!", "mert@acme.dev", "x"), "id"),
        (
            json!({ "id": "k", "thread": "k", "to": "*", "kind": "rm", "text": "x" }),
            "kind",
        ),
        (msg("e1", "mert@acme.dev", ""), "text"),
        (msg("e2", "mert@acme.dev", &"x".repeat(8001)), "text"),
        (msg("e3", "not-an-email", "x"), "email"),
    ] {
        let (st, v) = t
            .send(
                reqwest::Method::POST,
                &ta,
                &format!("/v1/messages{repo}"),
                body,
            )
            .await;
        assert_eq!(st, 400, "{why}");
        assert!(v["error"].as_str().unwrap().contains(why), "{v}");
    }
    assert_eq!(
        t.send(
            reqwest::Method::POST,
            &ta,
            "/v1/messages?repo=",
            msg("r1", "*", "x")
        )
        .await
        .0,
        400
    );

    // A revoked member is out.
    let mid = t.get(&tz, "/v1/me").await.1["member"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(t.relay.store.revoke_member(&mid).unwrap());
    assert_eq!(t.get(&tz, "/v1/me").await.0, 401);
    // Revoked invites stop working; members stay.
    t.relay.store.revoke_invites(&team.id).unwrap();
    assert_eq!(t.join(&invite, "New", "new@acme.dev").await.0, 403);
    assert_eq!(t.get(&tm, "/v1/me").await.0, 200);
}

#[tokio::test]
async fn long_polling_answers_the_moment_something_arrives() {
    let t = start().await;
    let team = t.relay.store.create_team("Acme").unwrap();
    let invite = t.relay.store.create_invite(&team.id, Some(1), "").unwrap();
    let ta = t.join(&invite, "Ayşe", "ayse@acme.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let tm = t.join(&invite, "Mert", "mert@acme.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let tz = t.join(&invite, "Zeynep", "zeynep@acme.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();

    let cursor = t.get(&tm, "/v1/events?after=0&wait=0").await.1["cursor"]
        .as_i64()
        .unwrap();
    // Nothing new: an empty answer after the wait.
    let started = Instant::now();
    let v = t
        .get(&tm, &format!("/v1/events?after={cursor}&wait=1"))
        .await
        .1;
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert_eq!(v["events"], json!([]));

    // Mert waits; Ayşe writes to him half a second later; his poll returns right then.
    let waiter = {
        let (http, base, tm) = (t.http.clone(), t.base.clone(), tm.clone());
        tokio::spawn(async move {
            let started = Instant::now();
            let v: Value = http
                .get(format!("{base}/v1/events?after={cursor}&wait=20"))
                .bearer_auth(&tm)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            (started.elapsed(), v)
        })
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    t.send(
        reqwest::Method::POST,
        &ta,
        "/v1/messages?repo=r",
        msg("q1", "mert@acme.dev", "hi"),
    )
    .await;
    let (took, v) = waiter.await.unwrap();
    assert!(took < Duration::from_secs(3), "{took:?}");
    assert_eq!(v["events"][0]["kind"], "message");
    assert_eq!(v["events"][0]["repo"], "r");
    let after = v["cursor"].as_i64().unwrap();
    assert!(after > cursor);

    // A message to Mert doesn't wake Zeynep; a note to everyone does; your own doesn't wake you.
    assert_eq!(
        t.get(&tz, &format!("/v1/events?after={cursor}&wait=0"))
            .await
            .1["events"],
        json!([])
    );
    t.send(
        reqwest::Method::POST,
        &ta,
        "/v1/messages?repo=r",
        json!({ "id": "n1", "thread": "n1", "to": "*", "kind": "note", "text": "hey" }),
    )
    .await;
    assert_eq!(
        t.get(&tz, &format!("/v1/events?after={after}&wait=0"))
            .await
            .1["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        t.get(&ta, &format!("/v1/events?after={after}&wait=0"))
            .await
            .1["events"],
        json!([])
    );
}

#[tokio::test]
async fn bodies_are_limited_and_old_things_are_purged() {
    let t = start().await;
    let team = t.relay.store.create_team("Acme").unwrap();
    let invite = t.relay.store.create_invite(&team.id, None, "").unwrap();
    let ta = t.join(&invite, "Ayşe", "ayse@acme.dev").await.1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let huge = json!({ "blob": "x".repeat(300 * 1024) });
    let (st, _) = t
        .send(reqwest::Method::PUT, &ta, "/v1/presence?repo=r", huge)
        .await;
    assert_eq!(st, 413);
    t.send(
        reqwest::Method::POST,
        &ta,
        "/v1/messages?repo=r",
        msg("q1", "*", "hi"),
    )
    .await;
    // Nothing is old enough yet; with a negative age everything is.
    assert_eq!(t.relay.store.purge(1).unwrap(), 0);
    let mid = t.get(&ta, "/v1/me").await.1["member"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!mid.is_empty());
    assert_eq!(
        t.get(&ta, "/v1/messages?repo=r").await.1["messages"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(t.get(&ta, "/healthz").await.0, 200);
}
