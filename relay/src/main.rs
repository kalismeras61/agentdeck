//! `agentdeck-relay`: run the relay, and manage its teams, invites, members and plans.

use agentdeck_relay::billing::{self, Billing, Plan};
use agentdeck_relay::{now_ms, serve, Relay, Store};
use std::path::PathBuf;

const HELP: &str = "AgentDeck Relay: instant team presence and messages for AgentDeck.

USAGE
  agentdeck-relay [--db FILE] <command>

COMMANDS
  serve [--addr HOST:PORT] [--retention-days N] [--open-signup]
                                                 Run the relay (default 127.0.0.1:8787, 30 days).
                                                 --open-signup lets anyone create a team from AgentDeck.
  team create <name>                             Make a team and print its first invite
  team list                                      Teams, members and plans
  invite create <team-id> [--days N] [--label L] A new invite code (shown once)
  invite revoke <team-id>                        Turn off every invite of a team
  members <team-id>                              A team's members
  member revoke <member-id>                      Sign a member out for good
  member admin <member-id> [--off]               Let a member manage the team's plan (or not)
  billing <team-id>                              A team's plan
  billing comp <team-id> [--seats N]             A complimentary plan (no payment; no seat limit
                                                 unless --seats)

OPTIONS
  --db FILE   The database (default: $AGENTDECK_RELAY_DB, else ./agentdeck-relay.db)

BILLING
  Off unless LEMONSQUEEZY_API_KEY, LEMONSQUEEZY_STORE_ID, LEMONSQUEEZY_VARIANT_ID and
  LEMONSQUEEZY_WEBHOOK_SECRET are set. Then new teams get a free trial (RELAY_TRIAL_DAYS, 14;
  RELAY_TRIAL_SEATS, 10) and pay per seat per month through Lemon Squeezy. Point Lemon Squeezy's
  webhook at https://<relay>/v1/billing/lemonsqueezy. See relay/README.md.

Put it behind a reverse proxy with TLS (Caddy, nginx); see relay/README.md.";

fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

fn flag(args: &mut Vec<String>, flag: &str) -> bool {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

fn date(ms: Option<i64>) -> String {
    match ms {
        Some(ms) => {
            let days = (ms - now_ms()) / 86_400_000;
            if days >= 0 {
                format!("in {days} day(s)")
            } else {
                format!("{} day(s) ago", -days)
            }
        }
        None => "-".into(),
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{HELP}");
        return;
    }
    let db = take(&mut args, "--db")
        .or_else(|| std::env::var("AGENTDECK_RELAY_DB").ok())
        .unwrap_or_else(|| "agentdeck-relay.db".into());
    let billing = match Billing::from_env() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let store = match Store::open(&PathBuf::from(&db)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Can't open {db}: {e}");
            std::process::exit(1);
        }
    };
    let words: Vec<String> = args.clone();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let result: Result<(), String> = match words.as_slice() {
        ["serve", ..] => {
            let addr = take(&mut args, "--addr").unwrap_or_else(|| "127.0.0.1:8787".into());
            let days = take(&mut args, "--retention-days").and_then(|d| d.parse().ok()).unwrap_or(30u32).max(1);
            let open = flag(&mut args, "--open-signup");
            tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| e.to_string()).and_then(|rt| {
                rt.block_on(async {
                    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| format!("Can't listen on {addr}: {e}"))?;
                    println!(
                        "agentdeck-relay on http://{addr} (database {db}, keeping {days} days, billing {}, signups {})",
                        if billing.is_some() { "on" } else { "off" },
                        if open { "open" } else { "by invite" }
                    );
                    tokio::select! {
                        r = serve(listener, store, days, billing, open) => r.map_err(|e| e.to_string()),
                        _ = tokio::signal::ctrl_c() => Ok(()),
                    }
                })
            })
        }
        ["team", "create", name @ ..] if !name.is_empty() => {
            let relay = Relay::with(store, billing, false);
            relay.create_team(&name.join(" ")).map_err(|e| e.to_string()).and_then(|t| {
                let code = relay.store.create_invite(&t.id, None, "first")?;
                println!("Team {} ({})\nInvite: {code}\n\nShare the relay's address and this invite with your team: in AgentDeck, Team view → connection → AgentDeck Relay. The first person to join manages the plan.", t.name, t.id);
                Ok(())
            })
        }
        ["team", "list"] => store.teams().map_err(|e| e.to_string()).map(|ts| {
            for (t, n) in ts {
                let p = store.plan(&t.id).unwrap_or_default();
                let status = if p.status.is_empty() { "-".into() } else { p.status };
                println!("{}\t{}\t{n} member(s)\t{status}\t{} seat(s)", t.id, t.name, p.seats.map(|s| s.to_string()).unwrap_or_else(|| "∞".into()));
            }
        }),
        ["invite", "create", team, ..] => {
            let team = team.to_string();
            let days = take(&mut args, "--days").and_then(|d| d.parse().ok());
            let label = take(&mut args, "--label").unwrap_or_default();
            store.create_invite(&team, days, &label).map(|c| println!("{c}"))
        }
        ["invite", "revoke", team] => store.revoke_invites(team).map_err(|e| e.to_string()).map(|n| println!("{n} invite(s) revoked")),
        ["members", team] => store.members(team).map_err(|e| e.to_string()).map(|ms| {
            for (m, seen) in ms {
                println!(
                    "{}\t{}\t{}\t{}{}",
                    m.id,
                    m.name,
                    m.email,
                    if m.revoked { "revoked".to_string() } else { seen.map(|s| format!("seen {}s ago", (now_ms() - s) / 1000)).unwrap_or_else(|| "never seen".into()) },
                    if m.admin { "\tadmin" } else { "" }
                );
            }
        }),
        ["member", "revoke", id] => store
            .revoke_member(id)
            .map_err(|e| e.to_string())
            .and_then(|ok| if ok { println!("Revoked"); Ok(()) } else { Err("No such member".into()) }),
        ["member", "admin", id, ..] => {
            let id = id.to_string();
            let on = !flag(&mut args, "--off");
            store
                .set_admin(&id, on)
                .map_err(|e| e.to_string())
                .and_then(|ok| if ok { println!("{}", if on { "Admin" } else { "Not admin" }); Ok(()) } else { Err("No such member".into()) })
        }
        ["billing", "comp", team, ..] => {
            let team = team.to_string();
            let seats = take(&mut args, "--seats").and_then(|s| s.parse().ok());
            store
                .set_plan(&team, &Plan { status: "comp".into(), seats, ..Default::default() })
                .map_err(|e| e.to_string())
                .and_then(|ok| if ok { println!("Complimentary plan for {team}"); Ok(()) } else { Err("No such team".into()) })
        }
        ["billing", team] => store.plan(team).map_err(|e| e.to_string()).and_then(|p| {
            let a = billing::access(billing.is_some(), &p, now_ms());
            let used = store.active_members(team).map_err(|e| e.to_string())?;
            let (sub, ..) = store.subscription(team).map_err(|e| e.to_string())?;
            println!(
                "status: {}\nseats: {used} used of {}\ntrial ends: {}\nrenews: {}\nends: {}\nsubscription: {}\ncan write: {}{}",
                if p.status.is_empty() { "-" } else { &p.status },
                p.seats.map(|s| s.to_string()).unwrap_or_else(|| "∞".into()),
                date(p.trial_ends_at),
                date(p.renews_at),
                date(p.ends_at),
                sub.unwrap_or_else(|| "-".into()),
                a.can_write,
                a.note.map(|n| format!("\nnote: {n}")).unwrap_or_default()
            );
            Ok(())
        }),
        _ => Err(format!("Unknown command. Run agentdeck-relay --help.\n\n{HELP}")),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
