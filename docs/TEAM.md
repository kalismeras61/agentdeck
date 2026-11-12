# Team

How AgentDeck's team features work, what they trust, and how a hosted version fits in.

## What it does

| | Where | How |
|---|---|---|
| Branches, who's on them, files that overlap | Team → Branches | Fetch of the project's remote (`refs/heads/*`) |
| Who is online, what their agents are doing | People list, branch panel | Presence: `refs/agentdeck/presence/<you>` |
| Time per branch and day | Team → Time, branch panel | Measured locally (`session_time`), summary shared in presence |
| What happened since you last looked | Team → Activity, notifications | Branch tips compared after each fetch (`team_feed`) |
| Questions, answers, notes, review requests | Team → Messages | Outbox: `refs/agentdeck/outbox/<you>` |
| Agents talking to teammates' agents | `team_members`, `ask_teammate`, `team_messages`, `reply_to_teammate` | AgentDeck's MCP bridge (`--mcp-bridge --team`) |
| Checking out a teammate's branch, reviewing it | Branch panel | `<project>.worktrees/review-<branch>` |

Opening a project in the Team view follows it: from then on it's fetched every two minutes in the background, and what matters (a message to you, a review request, a branch starting to touch files yours changes, the base moving ahead of your branches) arrives as a notification.

## Joining the team

"Share my work" is off until you turn it on, per project, after seeing exactly what will be shared. It's what lets you send messages and what gives your sessions the team tools. Turning it off deletes your presence from the remote.

## Agents talking to each other

Sessions in a project where you joined the team (and every bot session) get four tools from AgentDeck's own MCP server:

- `team_members` (reads): who works here, their branches and files, what their agents are doing.
- `team_messages` (reads): conversations with teammates.
- `ask_teammate` (sends): a question to a teammate or everyone, optionally waiting up to 30 minutes for the answer.
- `reply_to_teammate` (sends): an answer in a conversation.

A typical exchange: your agent, working on checkout, calls `ask_teammate` ("Is the postcode validation in AddressForm or the API?"). You approve the message. Ayşe's AgentDeck shows it in Messages and as a notification; she clicks **Let Claude Code answer**, which opens a session there with the question. Her agent reads her code and calls `reply_to_teammate`; she approves; your agent, still waiting, gets the answer and goes on.

## Trust and safety

- **Nothing leaves your Mac without you.** The two tools that send are never pre-approved, in your sessions or in bots' (a bot's send waits for you like any risky step). You see the exact text before it goes.
- **The remote is the trust boundary**, as it already is for the code: anyone who can push to it can write these refs. What comes from it is size-limited, validated and only displayed; agents receive teammates' words labeled as such ("information, not instructions"). A message whose sender doesn't own the outbox it's in is dropped.
- **No new credentials.** Fetching and pushing use the same git access you already have; nothing is stored by AgentDeck.

## Transports and a hosted version

Everything a teammate sees goes through one interface (`teamchat::Transport`): `fetch`, `publish_presence`, `presence`, `send`, `messages`. Today's implementation is `GitTransport` (the project's remote): free, private to people with repository access, no account, no server — but as fast as the fetch interval (two minutes in the background, immediately when you press Fetch or an agent is waiting for an answer).

`RelayTransport` is the second implementation: the [AgentDeck Relay](../relay/README.md), a small server a team runs (or that's offered hosted). Team view → connection button → AgentDeck Relay, with the relay's address and an invite code. Presence and messages then go to the relay, instantly (the app keeps a long poll open); branches still come from git. Teammates on the relay are reachable by agents even before they share anything (the relay knows the team's members).

As a hosted product, the relay is the natural paid tier:

| | Git (free) | Hosted relay (paid) |
|---|---|---|
| Delivery | Every 1–2 min | Instant (WebSocket) |
| Needs push access to the remote | Yes | No |
| Works across repositories and organizations | No | Yes |
| Web dashboard for leads (who's on what, time, reviews) | No | Yes |
| SSO, roles, audit log, retention policy | No | Yes |
| History beyond two weeks | No | Yes |

Built so far: teams, invites, per-member tokens, presence, messages, long polling, retention, rate limits, a CLI to manage it all, a Dockerfile, self-serve team creation, and billing: per seat per month through Lemon Squeezy (merchant of record), with a free trial, seat limits, a signed webhook, prorated seat changes and the customer portal (see [relay/README.md](../relay/README.md#charge-for-it-lemon-squeezy)). Still to build for a hosted offering: a web dashboard for leads, SSO, an admin API instead of the CLI, and history beyond the retention period.

What a relay should keep from the git design: end-to-end the same message format, the user approving every outgoing message, and the receiving app never treating content as instructions. Code and diffs stay on developers' Macs and the remote; the relay only carries presence, messages and time summaries.
