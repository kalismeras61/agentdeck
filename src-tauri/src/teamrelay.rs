//! The AgentDeck Relay as a team transport (see `relay/` for the server).
//!
//! A project connects to a relay by joining a team with an invite code. The relay gives back a
//! token (kept in the keychain, never in SQLite) and the app remembers the relay's address, the
//! team and the repository key under `team.relay.<project>`. From then on presence and messages
//! for that project go to the relay instead of `refs/agentdeck/*`; branches still come from git.
//!
//! Background updates use long polling: `wait` asks the relay for anything new and gets an
//! answer the moment it arrives (or after 25 seconds).

use crate::db::Db;
use crate::team::{self, Presence};
use crate::teamchat::{Message, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);
pub const WAIT_S: u64 = 25;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfig {
    pub url: String,
    pub team: String,
    pub repo: String,
    pub email: String,
    /// The last event the app has seen (long polling continues after it).
    #[serde(default)]
    pub cursor: i64,
}

fn key(project_id: &str) -> String {
    format!("team.relay.{project_id}")
}

fn token_name(project_id: &str) -> String {
    format!("team-relay:{project_id}")
}

pub fn config(db: &Db, project_id: &str) -> Option<RelayConfig> {
    db.meta(&key(project_id))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn save(db: &Db, project_id: &str, c: &RelayConfig) -> Result<(), String> {
    db.set_meta(
        &key(project_id),
        &serde_json::to_string(c).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// A relay address the app will send a token to: https, or http on this Mac only.
pub fn check_url(url: &str) -> Result<String, String> {
    let u = url.trim().trim_end_matches('/').to_string();
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| u == *p || u.starts_with(&format!("{p}:")) || u.starts_with(&format!("{p}/")));
    if (u.starts_with("https://") && u.len() > 8) || local {
        Ok(u)
    } else {
        Err(
            "Use the relay's https:// address (plain http only works for a relay on this Mac)."
                .into(),
        )
    }
}

/// The key a repository is known by on the relay: its remote without scheme, user or ".git"
/// ("github.com/acme/shop-app"), so every teammate's clone lands on the same one.
pub fn repo_key(remote_url: Option<&str>, fallback: &str) -> String {
    let Some(u) = remote_url.map(str::trim).filter(|u| !u.is_empty()) else {
        let name: String = fallback
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
            .collect();
        return format!("local/{}", if name.is_empty() { "project" } else { &name });
    };
    let rest = match u.split_once("://") {
        Some((_, r)) => r.to_string(),
        // scp-like: git@github.com:acme/shop.git
        None => u.replacen(':', "/", 1),
    };
    let rest = rest
        .rsplit_once('@')
        .map(|(_, h)| h.to_string())
        .unwrap_or(rest);
    let rest = rest
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_lowercase();
    rest.chars()
        .filter(|c| c.is_ascii_alphanumeric() || "._-/:~".contains(*c))
        .collect()
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

fn answer(r: reqwest::Result<reqwest::blocking::Response>, url: &str) -> Result<Value, String> {
    let r = r.map_err(|e| {
        if e.is_timeout() {
            format!("The relay at {url} didn't answer in time")
        } else {
            format!("Couldn't reach the relay at {url}: {e}")
        }
    })?;
    let status = r.status();
    let body: Value = r.json().unwrap_or(Value::Null);
    if status.is_success() {
        Ok(body)
    } else if status.as_u16() == 401 {
        Err(
            "The relay doesn't accept this Mac's token anymore (revoked?). Join the team again."
                .into(),
        )
    } else {
        Err(body["error"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("The relay answered {status}")))
    }
}

/// Joins a team with an invite; saves the token in the keychain and the rest for the project.
pub fn join(
    db: &Db,
    project_id: &str,
    url: &str,
    invite: &str,
    me: &team::Person,
    repo: &str,
) -> Result<RelayConfig, String> {
    let url = check_url(url)?;
    if me.email.trim().is_empty() {
        return Err(
            "Git doesn't know who you are in this repository. Set user.name and user.email first."
                .into(),
        );
    }
    let v = answer(
        client()?
            .post(format!("{url}/v1/join"))
            .json(&json!({ "invite": invite.trim(), "name": if me.name.is_empty() { &me.email } else { &me.name }, "email": me.email }))
            .send(),
        &url,
    )?;
    remember(db, project_id, url, &v, me, repo)
}

/// Creates a team on a relay that takes signups (a hosted one), with you as its admin.
/// Returns the config and the team's invite to share.
pub fn create_team(
    db: &Db,
    project_id: &str,
    url: &str,
    team_name: &str,
    me: &team::Person,
    repo: &str,
) -> Result<(RelayConfig, String), String> {
    let url = check_url(url)?;
    if me.email.trim().is_empty() {
        return Err(
            "Git doesn't know who you are in this repository. Set user.name and user.email first."
                .into(),
        );
    }
    let v = answer(
        client()?
            .post(format!("{url}/v1/teams"))
            .json(&json!({ "team": team_name.trim(), "name": if me.name.is_empty() { &me.email } else { &me.name }, "email": me.email }))
            .send(),
        &url,
    )?;
    let invite = v["invite"].as_str().unwrap_or("").to_string();
    Ok((remember(db, project_id, url, &v, me, repo)?, invite))
}

fn remember(
    db: &Db,
    project_id: &str,
    url: String,
    v: &Value,
    me: &team::Person,
    repo: &str,
) -> Result<RelayConfig, String> {
    let token = v["token"].as_str().ok_or("The relay sent no token")?;
    crate::secrets::set_named(&token_name(project_id), token)?;
    let c = RelayConfig {
        url,
        team: v["team"]["name"].as_str().unwrap_or("").to_string(),
        repo: repo.to_string(),
        email: v["member"]["email"]
            .as_str()
            .unwrap_or(&me.email)
            .to_string(),
        cursor: 0,
    };
    save(db, project_id, &c)?;
    Ok(c)
}

/// Leaves: takes your presence off the relay (when it can) and forgets the token.
pub fn leave(db: &Db, project_id: &str) -> Result<(), String> {
    if let (Some(c), Some(token)) = (
        config(db, project_id),
        crate::secrets::get_named(&token_name(project_id)),
    ) {
        let _ = client().map(|cl| {
            cl.delete(format!("{}/v1/presence", c.url))
                .query(&[("repo", &c.repo)])
                .bearer_auth(token)
                .send()
        });
    }
    crate::secrets::remove_named(&token_name(project_id));
    db.set_meta(&key(project_id), "").map_err(|e| e.to_string())
}

pub struct RelayTransport {
    pub project_id: String,
    pub root: String,
    pub config: RelayConfig,
    token: String,
    cache: Mutex<Option<(Vec<Presence>, Vec<Message>)>>,
}

impl RelayTransport {
    pub fn for_project(db: &Db, project_id: &str, root: &str) -> Option<Self> {
        let config = config(db, project_id)?;
        let token = crate::secrets::get_named(&token_name(project_id))?;
        Some(Self {
            project_id: project_id.into(),
            root: root.into(),
            config,
            token,
            cache: Mutex::new(None),
        })
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        answer(
            client()?
                .get(format!("{}{path}", self.config.url))
                .query(query)
                .bearer_auth(&self.token)
                .send(),
            &self.config.url,
        )
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        answer(
            client()?
                .post(format!("{}{path}", self.config.url))
                .bearer_auth(&self.token)
                .json(&body)
                .send(),
            &self.config.url,
        )
    }

    /// The team's plan, as the relay reports it (`GET /v1/billing`).
    pub fn billing(&self) -> Result<Value, String> {
        self.get("/v1/billing", &[])
    }

    /// A checkout page for `seats` seats (team admins).
    pub fn checkout(&self, seats: i64) -> Result<String, String> {
        self.post("/v1/billing/checkout", json!({ "seats": seats }))?["url"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "The relay sent no checkout address".into())
    }

    /// Changes the subscription's seats (team admins).
    pub fn set_seats(&self, seats: i64) -> Result<(), String> {
        self.post("/v1/billing/seats", json!({ "seats": seats }))
            .map(|_| ())
    }

    fn pull(&self) -> Result<(Vec<Presence>, Vec<Message>), String> {
        let repo = ("repo", self.config.repo.clone());
        let p = self.get("/v1/presence", std::slice::from_ref(&repo))?;
        let m = self.get("/v1/messages", &[repo])?;
        let presence = p["presence"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| team::parse_presence(v.to_string().as_bytes()))
                    .collect()
            })
            .unwrap_or_default();
        let messages = crate::teamchat::parse_outbox(m["messages"].to_string().as_bytes());
        let both = (presence, messages);
        if let Ok(mut c) = self.cache.lock() {
            *c = Some(both.clone());
        }
        Ok(both)
    }

    fn cached(&self) -> (Vec<Presence>, Vec<Message>) {
        if let Some(c) = self.cache.lock().ok().and_then(|c| c.clone()) {
            return c;
        }
        self.pull().unwrap_or_default()
    }

    /// Waits up to `WAIT_S` seconds for something new for this Mac; true when something came.
    pub fn wait(&self, db: &Db) -> Result<bool, String> {
        let cl = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(WAIT_S + 10))
            .build()
            .map_err(|e| e.to_string())?;
        let v = answer(
            cl.get(format!("{}/v1/events", self.config.url))
                .query(&[
                    ("after", self.config.cursor.to_string()),
                    ("wait", WAIT_S.to_string()),
                ])
                .bearer_auth(&self.token)
                .send(),
            &self.config.url,
        )?;
        let cursor = v["cursor"].as_i64().unwrap_or(self.config.cursor);
        let mine = v["events"].as_array().is_some_and(|e| {
            e.iter()
                .any(|x| x["repo"].as_str() == Some(self.config.repo.as_str()))
        });
        if cursor != self.config.cursor {
            let mut c = self.config.clone();
            c.cursor = cursor;
            save(db, &self.project_id, &c)?;
        }
        Ok(mine)
    }
}

impl Transport for RelayTransport {
    /// Branches from git (when there's a remote), presence and messages from the relay.
    fn fetch(&self) -> Result<(), String> {
        let git = team::remote_of(&self.root).map(|_| team::fetch(&self.root));
        self.pull()?;
        match git {
            Some(Err(e)) => Err(e),
            _ => Ok(()),
        }
    }
    fn publish_presence(&self, p: &Presence) -> Result<(), String> {
        let body = serde_json::to_value(p).map_err(|e| e.to_string())?;
        answer(
            client()?
                .put(format!("{}/v1/presence", self.config.url))
                .query(&[("repo", &self.config.repo)])
                .bearer_auth(&self.token)
                .json(&body)
                .send(),
            &self.config.url,
        )
        .map(|_| ())
    }
    fn unpublish_presence(&self, _email: &str) -> Result<(), String> {
        answer(
            client()?
                .delete(format!("{}/v1/presence", self.config.url))
                .query(&[("repo", &self.config.repo)])
                .bearer_auth(&self.token)
                .send(),
            &self.config.url,
        )
        .map(|_| ())
    }
    fn presence(&self) -> Vec<Presence> {
        self.cached().0
    }
    fn send(&self, _me: &team::Person, m: Message) -> Result<(), String> {
        let body = serde_json::to_value(&m).map_err(|e| e.to_string())?;
        answer(
            client()?
                .post(format!("{}/v1/messages", self.config.url))
                .query(&[("repo", &self.config.repo)])
                .bearer_auth(&self.token)
                .json(&body)
                .send(),
            &self.config.url,
        )?;
        if let Ok(mut c) = self.cache.lock() {
            *c = None;
        }
        Ok(())
    }
    fn messages(&self) -> Vec<Message> {
        self.cached().1
    }
    fn is_relay(&self) -> bool {
        true
    }
    fn members(&self) -> Vec<team::Person> {
        self.get("/v1/members", &[])
            .ok()
            .and_then(|v| v["members"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| {
                Some(team::Person {
                    name: m["name"].as_str()?.chars().take(80).collect(),
                    email: m["email"].as_str()?.to_lowercase(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_and_repository_keys() {
        assert!(check_url("https://relay.acme.dev/").is_ok());
        assert_eq!(
            check_url("https://relay.acme.dev/").unwrap(),
            "https://relay.acme.dev"
        );
        assert!(check_url("http://127.0.0.1:8787").is_ok());
        assert!(check_url("http://localhost").is_ok());
        for bad in [
            "http://relay.acme.dev",
            "ftp://x",
            "relay.acme.dev",
            "http://localhost.evil.dev",
            "https://",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
        for (url, want) in [
            (
                "https://github.com/Acme/Shop-App.git",
                "github.com/acme/shop-app",
            ),
            (
                "git@github.com:acme/shop-app.git",
                "github.com/acme/shop-app",
            ),
            (
                "ssh://git@github.com/acme/shop-app",
                "github.com/acme/shop-app",
            ),
            (
                "https://user:tok@gitlab.acme.dev/mobile/shop/",
                "gitlab.acme.dev/mobile/shop",
            ),
        ] {
            assert_eq!(repo_key(Some(url), "x"), want, "{url}");
        }
        assert_eq!(repo_key(None, "shop app!"), "local/shopapp");
    }

    /// A real relay on a free port, with one team; returns its address and an invite.
    fn relay() -> (String, String) {
        let relay = agentdeck_relay::Relay::new(agentdeck_relay::Store::open_in_memory().unwrap());
        let team = relay.store.create_team("Acme").unwrap();
        let invite = relay.store.create_invite(&team.id, None, "").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(format!("http://{}", l.local_addr().unwrap()))
                    .unwrap();
                axum::serve(l, agentdeck_relay::router(relay))
                    .await
                    .unwrap();
            });
        });
        (rx.recv().unwrap(), invite)
    }

    fn joined(dir: &std::path::Path, url: &str, invite: &str) -> (Db, String, String) {
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project("shop", &dir.to_string_lossy()).unwrap();
        let root = dir.to_string_lossy().into_owned();
        let me = team::me(&root);
        let c = join(&db, &p.id, url, invite, &me, "github.com/acme/shop").unwrap();
        assert_eq!(
            (c.team.as_str(), c.repo.as_str()),
            ("Acme", "github.com/acme/shop")
        );
        db.set_meta(&format!("team.share.{}", p.id), "1").unwrap();
        let s = db
            .create_session(&p.id, "claude-code", "default", "x")
            .unwrap();
        (db, p.id, s.id)
    }

    #[test]
    fn teammates_talk_through_a_relay_instantly() {
        use crate::teamchat::{call, transport_for};
        let (url, invite) = relay();
        let (_d, ayse, mert) = crate::team::tests::team();
        let (db_a, pid_a, s_a) = joined(&ayse, &url, &invite);
        let (db_m, pid_m, s_m) = joined(&mert, &url, &invite);
        assert!(join(
            &db_m,
            &pid_m,
            &url,
            "adi_wrong",
            &team::me(&mert.to_string_lossy()),
            "r"
        )
        .unwrap_err()
        .contains("invite"));

        // Presence goes to the relay, and comes back signed by the relay.
        let ra = ayse.to_string_lossy().into_owned();
        let rm = mert.to_string_lossy().into_owned();
        let ta = transport_for(&db_a, &pid_a, &ra);
        assert!(ta.is_relay());
        ta.publish_presence(&team::Presence {
            version: 2,
            name: "x".into(),
            email: "x@y".into(),
            branch: Some("feature/login-ui".into()),
            updated_at: 1,
            ..Default::default()
        })
        .unwrap();
        let tm = transport_for(&db_m, &pid_m, &rm);
        tm.fetch().unwrap();
        let o = crate::teamchat::overview_with(tm.as_ref(), &rm, 30).unwrap();
        assert_eq!(o.presence.len(), 1);
        assert_eq!(
            (
                o.presence[0].email.as_str(),
                o.presence[0].branch.as_deref()
            ),
            ("ayşe@team.dev", Some("feature/login-ui"))
        );

        // Mert's long poll returns the moment Ayşe's agent asks him something.
        let rt_m = RelayTransport::for_project(&db_m, &pid_m, &rm).unwrap();
        let _ = rt_m.wait(&db_m); // catch up with the presence above
        let started = std::time::Instant::now();
        // Mert never shared anything, but he is in the team: Ayşe's agent can reach him.
        let asked = call(
            &db_a,
            &s_a,
            "ask_teammate",
            &json!({ "to": "Mert", "question": "Is login on /v2?" }),
            60,
        )
        .unwrap();
        let thread = asked["thread"].as_str().unwrap().to_string();
        let rt_m = RelayTransport::for_project(&db_m, &pid_m, &rm).unwrap();
        assert!(
            rt_m.wait(&db_m).unwrap(),
            "a message to Mert wakes his poll"
        );
        assert!(started.elapsed() < Duration::from_secs(10));

        // Mert's agent reads and answers; Ayşe's agent, waiting, gets it.
        let seen = call(
            &db_m,
            &s_m,
            "team_messages",
            &json!({ "thread": thread }),
            60,
        )
        .unwrap();
        assert_eq!(seen["messages"][0]["text"], "Is login on /v2?");
        let answer = {
            let (db_m, s_m, thread) = (db_m, s_m, thread.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                call(
                    &db_m,
                    &s_m,
                    "reply_to_teammate",
                    &json!({ "thread": thread, "text": "Yes, /v2/session" }),
                    60,
                )
                .unwrap();
            })
        };
        let asked_again = call(
            &db_a,
            &s_a,
            "team_messages",
            &json!({ "thread": thread }),
            60,
        )
        .unwrap();
        assert_eq!(asked_again["messages"].as_array().unwrap().len(), 1);
        answer.join().unwrap();
        let back = call(
            &db_a,
            &s_a,
            "team_messages",
            &json!({ "thread": thread }),
            60,
        )
        .unwrap();
        let texts: Vec<_> = back["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(texts, ["Is login on /v2?", "Yes, /v2/session"]);

        // Nothing went into git refs.
        assert!(crate::git::git(&ra, &["for-each-ref", "refs/agentdeck"])
            .unwrap()
            .trim()
            .is_empty());

        // Leaving goes back to git.
        leave(&db_a, &pid_a).unwrap();
        assert!(!transport_for(&db_a, &pid_a, &ra).is_relay());
    }

    #[test]
    fn a_hosted_team_is_created_paid_for_and_stops_when_it_lapses() {
        use axum::{routing::post, Json};
        // Lemon Squeezy's checkout API, standing in.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async move {
                let ls = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let ls_url = format!("http://{}", ls.local_addr().unwrap());
                let app = axum::Router::new().route(
                    "/v1/checkouts",
                    post(|| async { Json(json!({ "data": { "attributes": { "url": "https://acme.lemonsqueezy.com/checkout/x" } } })) }),
                );
                tokio::spawn(async move { axum::serve(ls, app).await.unwrap() });
                let billing = agentdeck_relay::billing::Billing {
                    api_key: "k".into(),
                    store_id: "1".into(),
                    variant_id: "2".into(),
                    webhook_secret: "s".into(),
                    trial_days: 14,
                    trial_seats: 3,
                    redirect_url: None,
                    api_base: ls_url,
                };
                let relay = agentdeck_relay::Relay::with(agentdeck_relay::Store::open_in_memory().unwrap(), Some(billing), true);
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send((format!("http://{}", l.local_addr().unwrap()), relay.clone())).unwrap();
                axum::serve(l, agentdeck_relay::router(relay)).await.unwrap();
            });
        });
        let (url, relay) = rx.recv().unwrap();
        let (_d, ayse, mert) = crate::team::tests::team();
        let db = Db::open_in_memory().unwrap();
        let p = db.add_project("shop", &ayse.to_string_lossy()).unwrap();
        let me = team::me(&ayse.to_string_lossy());
        let (c, invite) =
            create_team(&db, &p.id, &url, "Acme", &me, "github.com/acme/shop").unwrap();
        assert_eq!(c.team, "Acme");
        assert!(invite.starts_with("adi_"));
        let tr = RelayTransport::for_project(&db, &p.id, &ayse.to_string_lossy()).unwrap();
        let b = tr.billing().unwrap();
        assert_eq!(
            (
                b["plan"]["status"].as_str(),
                b["admin"].as_bool(),
                b["used"].as_i64()
            ),
            (Some("trialing"), Some(true), Some(1))
        );
        assert_eq!(
            tr.checkout(3).unwrap(),
            "https://acme.lemonsqueezy.com/checkout/x"
        );
        assert!(tr.checkout(0).unwrap_err().contains("seats"));

        // Mert joins with the invite and isn't an admin.
        let db_m = Db::open_in_memory().unwrap();
        let pm = db_m.add_project("shop", &mert.to_string_lossy()).unwrap();
        join(
            &db_m,
            &pm.id,
            &url,
            &invite,
            &team::me(&mert.to_string_lossy()),
            "github.com/acme/shop",
        )
        .unwrap();
        let tm = RelayTransport::for_project(&db_m, &pm.id, &mert.to_string_lossy()).unwrap();
        assert_eq!(tm.billing().unwrap()["admin"], false);
        assert!(tm.checkout(3).unwrap_err().contains("admin"));

        // The trial ends: sending says why.
        let mut plan = relay
            .store
            .plan(&relay.store.teams().unwrap()[0].0.id)
            .unwrap();
        plan.trial_ends_at = Some(1);
        relay
            .store
            .set_plan(&relay.store.teams().unwrap()[0].0.id, &plan)
            .unwrap();
        let m = crate::teamchat::Message {
            id: "n1".into(),
            thread: "n1".into(),
            from: me.clone(),
            to: "*".into(),
            kind: "note".into(),
            text: "hi".into(),
            branch: None,
            agent: None,
            created_at: 1,
        };
        assert!(tr.send(&me, m).unwrap_err().contains("trial has ended"));
    }
}
