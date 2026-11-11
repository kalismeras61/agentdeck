//! `agentdeck-relay`: run the relay, and manage its teams, invites and members.

use agentdeck_relay::{serve, Store};
use std::path::PathBuf;

const HELP: &str = "AgentDeck Relay: instant team presence and messages for AgentDeck.

USAGE
  agentdeck-relay [--db FILE] <command>

COMMANDS
  serve [--addr HOST:PORT] [--retention-days N]   Run the relay (default 127.0.0.1:8787, 30 days)
  team create <name>                             Make a team and print its first invite
  team list                                      Teams and how many members each has
  invite create <team-id> [--days N] [--label L] A new invite code (shown once)
  invite revoke <team-id>                        Turn off every invite of a team
  members <team-id>                              A team's members
  member revoke <member-id>                      Sign a member out for good

OPTIONS
  --db FILE   The database (default: $AGENTDECK_RELAY_DB, else ./agentdeck-relay.db)

Put it behind a reverse proxy with TLS (Caddy, nginx); see relay/README.md.";

fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
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
    let store = match Store::open(&PathBuf::from(&db)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Can't open {db}: {e}");
            std::process::exit(1);
        }
    };
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let result: Result<(), String> = match words.as_slice() {
        ["serve", ..] => {
            let addr = take(&mut args, "--addr").unwrap_or_else(|| "127.0.0.1:8787".into());
            let days = take(&mut args, "--retention-days").and_then(|d| d.parse().ok()).unwrap_or(30u32).max(1);
            tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| e.to_string()).and_then(|rt| {
                rt.block_on(async {
                    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| format!("Can't listen on {addr}: {e}"))?;
                    println!("agentdeck-relay on http://{addr} (database {db}, keeping {days} days)");
                    tokio::select! {
                        r = serve(listener, store, days) => r.map_err(|e| e.to_string()),
                        _ = tokio::signal::ctrl_c() => Ok(()),
                    }
                })
            })
        }
        ["team", "create", name @ ..] if !name.is_empty() => store.create_team(&name.join(" ")).map_err(|e| e.to_string()).and_then(|t| {
            let code = store.create_invite(&t.id, None, "first")?;
            println!("Team {} ({})\nInvite: {code}\n\nShare the relay's address and this invite with your team: in AgentDeck, Team view → Connection → AgentDeck Relay.", t.name, t.id);
            Ok(())
        }),
        ["team", "list"] => store.teams().map_err(|e| e.to_string()).map(|ts| {
            for (t, n) in ts {
                println!("{}\t{}\t{n} member(s)", t.id, t.name);
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
                    "{}\t{}\t{}\t{}",
                    m.id,
                    m.name,
                    m.email,
                    if m.revoked { "revoked".to_string() } else { seen.map(|s| format!("seen {}s ago", (agentdeck_relay::now_ms() - s) / 1000)).unwrap_or_else(|| "never seen".into()) }
                );
            }
        }),
        ["member", "revoke", id] => store
            .revoke_member(id)
            .map_err(|e| e.to_string())
            .and_then(|ok| if ok { println!("Revoked"); Ok(()) } else { Err("No such member".into()) }),
        _ => Err(format!("Unknown command. Run agentdeck-relay --help.\n\n{HELP}")),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
