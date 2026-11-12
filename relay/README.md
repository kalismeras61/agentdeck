# AgentDeck Relay

Instant team presence and messages for [AgentDeck](../README.md), on a server you run.

Without a relay, AgentDeck's team features travel through the project's git remote (`refs/agentdeck/*`): free and serverless, but only as fast as a fetch and only for people who can push. A relay makes it instant (long polling: a message arrives within a second), needs no push access, and serves any number of repositories per team.

It carries presence (who is online, on which branch, what their agents are doing, time per branch) and messages (questions, answers, notes, review requests). **Never code, diffs or files**: those stay in git.

## Run it

```bash
cd relay
cargo build --release
./target/release/agentdeck-relay --db /var/lib/agentdeck/relay.db team create "Acme Mobile"
# Team Acme Mobile (t_…)
# Invite: adi_…
./target/release/agentdeck-relay --db /var/lib/agentdeck/relay.db serve --addr 127.0.0.1:8787
```

Or with Docker:

```bash
docker build -t agentdeck-relay relay/
docker run -d --name relay -p 127.0.0.1:8787:8787 -v relay-data:/data agentdeck-relay
docker exec relay agentdeck-relay team create "Acme Mobile"
```

Put it behind TLS. AgentDeck only sends its token to `https://` addresses (plain `http://` only for a relay on the same Mac). With Caddy:

```
relay.acme.dev {
    reverse_proxy 127.0.0.1:8787
}
```

Long polls stay open up to 30 seconds; give your proxy a read timeout above that (Caddy's default is fine; nginx: `proxy_read_timeout 60s;`).

## Join from AgentDeck

Team view → the connection button next to the title (it says "Git remote") → **AgentDeck Relay**: the relay's address and the invite code. Everyone joins with their own name and email from git and gets their own token, kept in their keychain.

## Charge for it (Lemon Squeezy)

A relay can sell itself: teams pay per seat per month through [Lemon Squeezy](https://www.lemonsqueezy.com), which is the merchant of record (it charges VAT and sales tax worldwide and sends the invoices). Billing is off unless these are set:

| Variable | What |
|---|---|
| `LEMONSQUEEZY_API_KEY` | An API key (Settings → API) |
| `LEMONSQUEEZY_STORE_ID` | Your store's id |
| `LEMONSQUEEZY_VARIANT_ID` | The variant of a subscription product priced **per unit** (one unit = one seat, monthly) |
| `LEMONSQUEEZY_WEBHOOK_SECRET` | The signing secret of a webhook pointing at `https://<relay>/v1/billing/lemonsqueezy`, with the `subscription_*` events |
| `RELAY_TRIAL_DAYS`, `RELAY_TRIAL_SEATS` | The free trial (default 14 days, up to 10 people) |
| `RELAY_CHECKOUT_REDIRECT` | Optional: where people land after paying |

Run it with `serve --open-signup` to let anyone create a team from AgentDeck (Team view → connection → AgentDeck Relay → Create a team). Without it, you create teams with the CLI.

How it goes:

1. A new team starts on the free trial. Its first member is its admin.
2. The admin opens **Plan** in AgentDeck, picks the seats and subscribes: the relay makes a checkout with the team's id in it, AgentDeck opens it in the browser, Lemon Squeezy takes the payment.
3. Lemon Squeezy's webhook (checked against the signing secret) tells the relay the subscription's status, seats, renewal date and the customer portal's address. Admins change seats from AgentDeck (prorated) and reach invoices and the card in the portal.
4. While the trial or the subscription is good, everything works; a failed payment shows a warning while Lemon Squeezy retries; a cancelled subscription runs to the end of its period. After that, or when the trial ends unpaid, the team can still read but can't share, send or add people until an admin subscribes. Joining needs a free seat; the same person on another Mac doesn't take a second one.

`agentdeck-relay billing <team-id>` shows a team's plan; `billing comp <team-id> [--seats N]` gives one for free; `member admin <member-id>` makes someone an admin.

## Manage

```
agentdeck-relay team list
agentdeck-relay invite create <team-id> [--days 7] [--label "new hires"]
agentdeck-relay invite revoke <team-id>        # every invite; members stay
agentdeck-relay members <team-id>
agentdeck-relay member revoke <member-id>      # their token stops working at once
```

`--db FILE` (or `AGENTDECK_RELAY_DB`) picks the database; `serve --retention-days N` (default 30) is how long messages and stale presence are kept.

## Security

- Tokens and invite codes are random (256 bits) and stored only as SHA-256 hashes.
- The relay decides who sent what: a message's sender and a presence's name and email are always the member's own.
- A member reads their team's presence and the messages they sent, received, or that went to everyone. Other teams see nothing.
- Requests are limited to 256 KB, messages to 8000 characters, writes to 120 a minute per member, joins to 30 a minute.
- In AgentDeck every outgoing message still waits for the user's approval, and what arrives is shown and given to agents as a teammate's words, never as instructions.

## API

| | |
|---|---|
| `POST /v1/join` `{invite, name, email}` | → `{token, team, member}` |
| `GET /v1/me` · `GET /v1/members` | who you are; your team |
| `PUT` · `GET` · `DELETE /v1/presence?repo=R` | your presence in a repository; everyone's |
| `POST` · `GET /v1/messages?repo=R[&since=MS]` | send; read |
| `GET /v1/events?after=SEQ&wait=S` | long poll: `{cursor, events: [{seq, repo, kind}]}` |
| `POST /v1/teams` `{team, name, email}` | a new team, you as its admin (`--open-signup` only) |
| `GET /v1/billing` | the team's plan, seats and what it allows |
| `POST /v1/billing/checkout` · `POST /v1/billing/seats` `{seats}` | subscribe; change seats (admins) |
| `POST /v1/billing/lemonsqueezy` | Lemon Squeezy's webhook (signed) |
| `GET /healthz` | `ok` |

Everything but join and health takes `Authorization: Bearer <token>`. `repo` is the repository's remote without scheme, user or `.git` (`github.com/acme/shop-app`), so every clone of it meets in the same place.
