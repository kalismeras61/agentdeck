# Bots

A bot is an agent that stays: a name, a job, instructions, an agent and model, a folder to work in, a memory file it keeps, and a queue of tasks it works through one at a time on your Mac, while you do something else. You give it work from AgentDeck (later also on a schedule, from another bot, from your phone or from Telegram), and it reports back.

Bots are built from what AgentDeck already has: each bot owns one ongoing session (any agent: Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity, your own models), so its conversation, diffs, approvals, questions, model and effort pickers all work as in any session.

## Milestones

1. **Bots** (this document's first part): create and edit bots, the task queue (queued → running → done / failed / cancelled; pause, resume, cancel, retry), the memory file, unattended approvals, a Bots section in the sidebar and a bot page.
2. **Bots of their own, schedules and a manager bot**: a bot gets its own workspace instead of belonging to one project, and each task can say which project to work in; tasks that repeat ("weekdays at 9:00", "every 6 hours"); a bot can hand tasks to other bots and use their results, through an AgentDeck MCP server the agents call (`list_bots`, `delegate_task`, `wait_for_tasks`).
3. **Remote**: a page for your phone served by AgentDeck on your Mac (local network or Tailscale, paired with a code), and Telegram (your own bot token, paired chats only). Approvals and questions can be answered there too.

## 1. Bots

### Data (SQLite, migrations like the existing ones in `db.rs`)

`bots`
| column | |
|---|---|
| `id` TEXT PK | |
| `name` TEXT | shown everywhere, unique per workspace (case-insensitive) |
| `role` TEXT | one line: its job ("Reviews pull requests in the shop repo") |
| `instructions` TEXT | longer guidance, may be empty |
| `provider` TEXT, `model` TEXT, `effort` TEXT NULL | as sessions |
| `project_id` TEXT | the folder it works in (an AgentDeck project). Required. |
| `session_id` TEXT NULL | its current session; created on first task, kept for good. "Start fresh" makes a new one. |
| `color` TEXT | one of a few accent names, for its mark |
| `paused` INTEGER | a paused bot keeps its queue but starts nothing new |
| `created_at`, `updated_at` INTEGER | |

`bot_tasks`
| column | |
|---|---|
| `id` TEXT PK, `bot_id` TEXT | |
| `text` TEXT | what to do |
| `source` TEXT | `you` (later `schedule`, `bot:<id>`, `web`, `telegram`) |
| `status` TEXT | `queued`, `running`, `done`, `failed`, `cancelled` |
| `result` TEXT NULL | the bot's final reply for this task |
| `error` TEXT NULL | why it failed |
| `created_at`, `started_at`, `finished_at` INTEGER | |
| `parent_id` TEXT NULL | for delegated tasks (milestone 2) |

Deleting a bot deletes its tasks; its session stays as an ordinary session (renamed "<name> (deleted bot)"). (Since milestone 2 the bot's `project_id` is an optional default project and its sessions are in `bot_sessions`; see "Milestone 2 as built" below.)

### Runner (`src-tauri/src/bots.rs`)

- One task runs per bot at a time. When a task is added, or a task ends, or a bot is resumed: if the bot isn't paused and has no running task, the oldest queued task starts.
- Starting a task: make sure the bot's session exists (create it in the bot's project with the bot's provider, model and effort; title = bot name), then send it the task as a message. The first message of a new session carries the bot's brief (below); later tasks carry a short header so the agent knows it's a new task.
- A task ends when the bot's session ends a turn (`relay::turn_end`, the same signal workflows use): `Reply` → `done` with the reply as `result`; `Failed` → `failed` with the reason; `NoReply` → `done` with an empty result. Wire it next to `r.on_event / f.on_event` in `lib.rs`.
- If AgentDeck restarts with a task `running`, it goes back to `queued` (it starts again when the bot's turn is free), like `recover_after_restart`.
- Cancelling a running task stops the agent's turn (the existing stop/interrupt path) and marks it `cancelled`; a queued one is just marked `cancelled`.
- You can still write to the bot's session directly; that's an ordinary message, not a task, and a running task's turn end is only matched to the task it started (keep the task id on the bot while it runs; ignore turn ends while nothing runs).

### The brief (first message of a bot's session)

```
You're {name}, a bot in AgentDeck. Your job: {role}.
{instructions}

You work through tasks one at a time. Each task arrives as a message starting with "Task". When you're done, end with a short report of what you did and what's left; that report is the task's result.

Your memory file is {memory path}. Read it at the start of every task. When you learn something worth keeping for later tasks (decisions, conventions, where things are, what the user prefers), add it there in a line or two. Keep it short and current; remove what's no longer true.
```
Later tasks: `Task from {source} ({date time}):\n{text}` (a reminder line about the memory file is fine).

### Memory

- File: `{project folder}/.agentdeck/bots/{slug of name}.md`, created with a heading when the bot is created. Inside the project so agents can write it without an "outside the project" hold.
- Add `.agentdeck/` to the project's `.git/info/exclude` (not `.gitignore`) when creating it, so it never shows up in diffs or commits; skip when the folder isn't a git repo.
- The bot page shows the memory file and lets you edit it (reuse the Agent setup text editor component if it fits).

### Approvals

Bots run unattended, so they get the workflow's rule: ordinary approval requests are allowed for them; risky ones (`flow::risky`) or ones reaching outside the bot's folder (`flow::outside`) wait for you, with the same `flow_needs_you`-style notice (a `bot_needs_you` event) and the bot shows "Needs you" in the sidebar. Questions (AskUserQuestion, request_user_input) always wait for you. Factor the shared logic out of `flow.rs` rather than copying it.

### Commands (Tauri, `lib.rs`; frontend wrappers in `src/lib/live.ts`)

`list_bots`, `save_bot(bot)` (create or update; validates name, project), `delete_bot(id)`, `bot_tasks(bot_id, limit)`, `add_bot_task(bot_id, text)`, `cancel_bot_task(id)`, `retry_bot_task(id)` (a failed or cancelled task goes back to the queue), `set_bot_paused(id, paused)`, `restart_bot_session(id)` (next task starts a new session), `read_bot_memory(id)`, `write_bot_memory(id, text)`.
Events: `agentdeck://bot` (a bot row changed, with its counts: queued, running task id), `agentdeck://bot-task` (a task row changed).

### Interface

- **Sidebar**: a "Bots" section above the projects: each bot with its color mark, name and a status line ("Working on: …", "3 queued", "Paused", "Needs you", "Idle"); "New bot". Clicking opens the bot page. Bots' own sessions don't also appear under their project (filter them out there).
- **Bot page** (a view like Overview/Settings, keyed by bot id):
  - Header: mark, name, role; agent · model · effort; Pause/Resume; Edit; "…" menu (Start fresh, Delete).
  - Main: the bot's session conversation (reuse `Conversation`), with a composer whose send adds a task ("Give {name} a task…"); a toggle or secondary action sends a plain message instead.
  - Side panel with tabs: **Queue** (running task on top with elapsed time; queued tasks with cancel; recent done/failed/cancelled with their result folded, retry), **Memory** (the file, editable).
- **Bot editor** (dialog): name, role, instructions, agent (only installed ones), model and effort (reuse the model menu pieces), project folder (existing projects, or add a folder), color. Creating a bot from a project's "…" menu prefills the folder.
- Empty state on the Bots section when there are none: one line on what a bot is and "New bot".
- All text through `t()` / `tn()` (docs/I18N.md), with translations for all 10 catalogs.

### Tests

Rust: migrations; queue order and one-at-a-time; turn end → done/failed; restart recovery; cancel; pause/resume; memory path and git exclude; approval rule for bot sessions (allowed vs held). Frontend: store/bot helpers. An end-to-end test with the mock Claude (as the existing ignored integration tests do) that runs two queued tasks in order is welcome.

## 2. Bots of their own, schedules and a manager bot

### 2a. A bot's own workspace

Bots no longer belong to one project (OpenBot works the same way: each agent has its own workspace).

- **Workspace**: every bot gets a folder of its own, `~/AgentDeck/Bots/<Name>/` (created on save; the name's characters made safe for a folder; renaming the bot renames the folder). It's where the bot works when a task names no project, where it can keep files it produces, and where its memory lives: `<workspace>/MEMORY.md`.
- **Projects per task**: a task can name a project (`bot_tasks.project_id`, NULL = the bot's workspace). The bot's editor has an optional **default project** (the old `project_id`, now nullable), used when you don't pick one. The task box on the bot page has a project picker ("In: its workspace / <project>"), defaulting to the bot's default.
- **One session per place**: a bot keeps one ongoing session per folder it works in (`bot_sessions (bot_id, project_id NULL, session_id)`), so it remembers what it did in each project, and the memory file carries what it learned across all of them. The bot page shows the session of the selected place (a place switcher next to the title: Workspace, and each project it has worked in); the queue shows each task's place.
- **Sessions need a project row**: the workspace is registered as a hidden project (`projects.hidden = 1`, never listed in the sidebar, project pickers or imports) so sessions, diffs and approvals keep working unchanged.
- **The brief** names the workspace and the memory file's absolute path, and says a task may name a project folder to work in. Each task header names its folder: `Task from {source} in {folder} ({date time}):`.
- **Unattended rule**: while a task runs, the bot may reach the task's folder and its own workspace (memory included); anything else outside them waits for you, as before.
- **Migration**: existing bots keep their project as the default project; their memory file moves (copied, then the old one removed) to the new workspace's `MEMORY.md`; their current session becomes the session for that project.
- **Creating a bot** no longer requires a folder. The sidebar's per-project bot button still prefills the default project.

### 2b. Schedules

`bot_schedules`: `id`, `bot_id`, `text`, `project_id` NULL, `kind` (`daily` | `interval`), `times` (for daily: "HH:MM" list, local time), `weekdays` (for daily: set of 1–7, empty = every day), `every_minutes` (for interval, at least 15), `enabled`, `last_run_at`, `next_run_at`, `created_at`.

- A scheduler thread wakes every 30 s and enqueues each due schedule as a task (`source = "schedule"`, same project), then computes the next run. Runs missed while AgentDeck or the Mac was off aren't made up; the next run is computed from now.
- A schedule whose bot is paused still adds its task (it waits in the queue); a schedule never adds a second task while its previous one is still queued or running (it skips that run and says so on the schedule's row: `last_task_id`, `skipped_at`).
- Local time needs a time zone: use the `chrono` crate (`Local`) or equivalent; daylight-saving changes must not double-run or skip a day.
- Interface: a **Schedules** tab on the bot page (list with next run time, on/off switch, edit, delete; "Run now" adds the task immediately) and a small editor: what to do, where (project picker), when (daily at times on chosen weekdays, or every N hours/minutes). The sidebar status mentions the next run when the bot is idle ("Next: 09:00").

### 2c. A manager bot (bots handing work to bots)

Agents can't call AgentDeck directly, so AgentDeck gives every bot session an MCP server named `agentdeck` with these tools:

| tool | |
|---|---|
| `list_bots()` | every bot: name, job, agent, status, its default project |
| `list_projects()` | the projects (name, folder) bots can be pointed at |
| `delegate_task(bot, task, project?)` | adds a task to another bot's queue (`source = "bot:<id>"`, `parent_id` = the caller's running task) and returns its id at once |
| `wait_for_tasks(task_ids, timeout_seconds?)` | waits (up to the timeout, default 600, max 1800) until those tasks finish; returns each one's status and result |
| `task_status(task_ids)` | the same without waiting |

- **Transport**: a stdio MCP server, run as the AgentDeck binary itself in a bridge mode (`<app binary> --mcp-bridge`, checked at the very start of `main` before Tauri starts). The bridge relays JSON-RPC to the running app over a Unix socket in the app data folder (permissions 0700/0600), authenticated with a per-session random token given to the bridge in its environment. It never runs anything itself; every tool call is executed by the app.
- **Wiring per agent**: Claude Code: `--mcp-config` with a JSON file written to the app data folder for that session (plus `--allowedTools mcp__agentdeck__*` or the equivalent so the bot isn't asked about its own tools); Codex: `-c mcp_servers.agentdeck.command=… args=… env=…` (check the installed Codex's config schema); Gemini CLI, Grok Build and Kimi Code over ACP: the `mcpServers` list in `session/new` / `session/load` (stdio entry). Antigravity and API models: skip if their CLI has no way to add an MCP server, and say so in the bot editor ("can't hand work to other bots"). Only bot sessions get the server, not ordinary sessions.
- **Rules**: a bot can't delegate to itself; a delegated task can delegate further only to depth 3; a task can't wait on a task that is waiting on it (refuse cycles); `wait_for_tasks` returns early with what's done when the caller's task is cancelled. Cancelling a task cancels its queued children too.
- **The manager's brief** gets one more paragraph when other bots exist: "You can hand tasks to other bots with the agentdeck tools: list_bots, delegate_task, then wait_for_tasks to get their results. Use them when another bot's job fits a part of the work better."
- **Interface**: a delegated task shows "from <bot>" in the target's queue; the parent task shows its children with their status (and links to them) in the queue; the conversation shows the tool calls like any other tool call.

## Milestone 2 as built

What the code does now (`bots.rs`, `schedule.rs`, `mcp.rs`, `unattended.rs`, `db.rs` schema 9, `lib.rs`; frontend `src/lib/bots.ts`, `src/lib/agentdeckTools.ts`, `src/views/BotPage.tsx`, `src/components/bots/*`).

### 2a. Workspaces and places

- Every bot has `~/AgentDeck/Bots/<Name>/` (registered as a hidden project), with `MEMORY.md` in it. Renaming the bot renames the folder (refused while a task works in the workspace, or when a folder of that name is already there); Claude Code conversations there are copied to the new folder so they resume, Codex resumes anywhere, the other agents start over with the brief. A name whose folder would clash with another bot's ("PR/ Reviewer" vs "PR: Reviewer" is fine, "PR/ Reviewer" vs "PR- Reviewer" isn't) is refused.
- A task works in a project (`bot_tasks.project_id`) or in the workspace (NULL). The bot keeps one session per place (`bot_sessions`); the bot page has a place switcher (its workspace, each project it has worked in, and earlier sessions it let go of), and each queued task shows its place.
- Milestone 1 bots: their project becomes their default project, their memory file is copied to `MEMORY.md` and the old one removed (with the empty `.agentdeck/bots` folders), and their session becomes their session for that project (no second brief).
- **Design changes from the plan above:**
  - **Deleting a bot** deletes its tasks and schedules; its sessions stay as ordinary sessions ("<name> (deleted bot)"), and its workspace folder stays. If the bot has any sessions, its workspace becomes an ordinary (visible) project so those sessions can still be found in the sidebar; a bot that never worked leaves no project behind.
  - **Removing a project no longer deletes bots.** Tasks running there fail ("Its project was removed"), queued ones fail when they come up, schedules for it are turned off, and bots that had it as their default project work in their workspace by default.
  - Changing a bot's agent lets its old sessions go (renamed "<name> (earlier)"); changing its name, model or effort carries over to its sessions.

### 2b. Schedules

- `schedule.rs` works out the next run with `chrono` in local time (daily at "HH:MM" times on chosen weekdays, or every N minutes, at least 15); a time that falls in the gap when clocks go forward runs an hour later that day instead of being skipped, and a time that happens twice (clocks going back) runs once, at its first occurrence.
- The scheduler ticks every 30 s. A run missed by more than two minutes (AgentDeck or the Mac was off) isn't made up; the next run is counted from now. A run whose previous task is still queued or running is skipped and the schedule row says so ("Skipped the 09:00 run: the one before was still queued or working"). "Run now" adds the task at once without moving the next run, and is refused while the last run is still going.
- The sidebar's status line for an idle bot shows its next run ("Next: Tue 09:00").

### 2c. The manager bot (`mcp.rs`)

- **Bridge**: `main.rs` checks for `--mcp-bridge` before anything else and runs `mcp::bridge_main`: MCP over stdio (JSON-RPC 2.0, one message per line): `initialize` (answers with the client's protocol version when it's one of 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05, else the newest), `ping`, `tools/list` (static, so the tools list even when the app can't be reached), `tools/call` (each call on its own thread, so a long wait doesn't block other calls); notifications are ignored, other methods get -32601, unknown tools -32602. A tool call is one line to the app's socket (`{"token","tool","args"}`) and one line back; it runs nothing itself. Errors from the app (refusals, unknown token, app not running) come back as tool results with `isError: true` and a plain sentence the model can act on ("AgentDeck isn't running (or was restarted), so the agentdeck tools can't reach it…").
- **App side (`Hub`)**: listens on `<app data>/mcp/agentdeck.sock` (folder 0700, socket 0600; a private folder in the temp folder when that path is too long for a Unix socket). Each start of a bot session's agent process gets a new 256-bit token (two OS-random UUIDs) mapped in memory to (bot, session, agent); the previous token of that session stops working, and a token whose session no longer belongs to its bot is refused. Tokens are never written to SQLite or the log. The executable is the app's own (`current_exe`; debug builds can point `AGENTDECK_BRIDGE_BIN` elsewhere).
- **Wiring** (only bot sessions, and only while the hub is up):
  - Claude Code: `--mcp-config <app data>/mcp/sessions/<session>.json` (0600; the server with its token and socket in `env`; files from a previous run are removed at start) and `--allowedTools mcp__agentdeck__list_bots,…` so the bot is never asked about its own tools. The user's own MCP servers stay.
  - Codex: `-c mcp_servers.agentdeck.command=… args=["--mcp-bridge"] env_vars=["AGENTDECK_MCP_TOKEN","AGENTDECK_MCP_SOCKET"] default_tools_approval_mode="approve" tool_timeout_sec=1920 startup_timeout_sec=20`. The token and socket are set in Codex's own environment and passed on to the server through `env_vars`, so they never appear in a process list.
  - Gemini CLI, Grok Build, Kimi Code (ACP): a stdio entry in `mcpServers` of `session/new` and `session/load` (with the token in its `env`). Gemini CLI only starts stdio MCP servers in folders it trusts, so AgentDeck sets `GEMINI_CLI_TRUST_WORKSPACE=true` for a Gemini bot working in its own workspace (a folder AgentDeck made); in a project, the user's own Gemini CLI trust settings decide. Gemini CLI asks before each MCP call; the bot runner allows the agentdeck ones.
  - Antigravity and the API agent: no server (agy's headless mode has no way to add one; the API agent has no MCP client). The bot editor says "Antigravity can't hand work to other bots. Other bots can still hand work to it."
- **Tools**: `list_bots` (name, id, job, agent, status, default project, `you` for the caller), `list_projects` (projects you added; bots' workspaces aren't listed), `delegate_task(bot, task, project?)` (bot and project by name, case-insensitive, or id; `project` may also be a folder path or "workspace"; without it the target's default project), `wait_for_tasks(task_ids, timeout_seconds?)` (default 600, at most 1800; ACP agents at most 540, since Gemini CLI gives an MCP call 10 minutes), `task_status(task_ids)`. At most 20 ids per call. Results are compact JSON text (`{"task_id","bot","status","works_in","note"?}`, `{"tasks":[{"id","bot","status","result"|"error"}],"all_finished","note"?}`).
- **Rules**: a delegated task gets `source = "bot:<caller bot id>"` and `parent_id` = the caller's running task (none when the call comes from a message you wrote to the bot yourself). A bot can't delegate to itself, nor to any bot that owns a task above it in the chain (that bot would end up waiting on itself: A → B → A deadlocks while A's task waits). A task at depth 3 (handed on three times) can't hand on further. `wait_for_tasks` refuses waiting on your own task, on a task above yours, or on one that is (directly or through other waits) waiting on yours, and returns early, with a note, when the caller's task is cancelled or ends. Cancelling a task cancels its children that haven't started; children already working go on.
- **Brief**: when other bots exist, the hub is up and the bot's agent takes the server, the brief ends with the paragraph above. A delegated task's header says who it's from: `Task from Lead (another bot) in /path (…)`.
- **Approvals**: requests for the agentdeck tools (Claude Code `mcp__agentdeck__*`, Codex `mcpToolCall` on server `agentdeck`, ACP calls titled "<tool> (agentdeck MCP Server)") are allowed for a bot's running task without the risky/outside checks, so a task text that mentions `git push` isn't held.
- **Interface**: the parent task (running card, or a finished one when opened) lists the tasks it handed on: each bot's mark and name, its status, what it was asked; clicking one opens that bot's page at the task's place. A delegated task shows "from <bot>" (with the bot's mark; a link in the running card and the queue); its conversation label is "Task from <bot>". Tool calls show as "Listed the bots", "Delegated to Docs Writer" (with the task's first line), "Waited for 2 tasks", "Checked 1 task" in Claude Code, Codex and ACP sessions (`src/lib/agentdeckTools.ts`). `bot_tasks` rows carry a `children` count; `bot_task_children(task_id)` lists them.

### Tests (milestone 2)

Workspace creation, rename and migration of existing bots; per-place sessions; the unattended rule with workspace and task folder; schedule next-run computation (daily with weekdays, interval, DST change, missed runs), no double-enqueue; the MCP bridge (handshake, token check, each tool, refusal of self-delegation, depth and cycles), and an integration test with the real `claude` and the mock API in which a manager bot delegates a task to a second bot and uses its result.

As built: `mcp::tests` (11: handshake and version negotiation, tools/list, unknown tool and method, parse errors, bad/missing token and app not running, tokens per session and file/socket permissions, each tool, names and ids, default project, self-delegation, depth 3, cycles up the chain and through waits, wait timeout/results/early return on cancel, cancel cascade, the brief paragraph, per-agent argument shapes, approval detection) and three integration tests run against the real CLIs (`cargo build` first: the bridge is the app binary):

```bash
python3 scripts/mock_anthropic.py 8787 & python3 scripts/mock_openai.py 8788 & python3 scripts/mock_gemini.py 8789 &
(cd src-tauri && CARGO_INCREMENTAL=0 cargo build && AGENTDECK_IT=1 cargo test --lib mcp::integration -- --ignored --test-threads=1)
```

- `a_manager_bot_hands_a_task_to_another_bot_and_uses_its_result`: real Claude Code 2.1.316; the manager loads the tools with ToolSearch, calls `delegate_task` and `wait_for_tasks` without being asked, the second bot does the task, the manager's result quotes it; the config file is 0600 and the token isn't in the database.
- `a_codex_bot_reaches_the_agentdeck_tools`: real Codex 0.166.0 starts the bridge from the `-c` keys, gets the token through `env_vars`, calls `list_bots` and `delegate_task` with no approval request.
- `a_gemini_bot_reaches_the_agentdeck_tools`: real Gemini CLI 0.66 over ACP starts it from `mcpServers`, asks before `list_bots`, the bot runner allows it.

## Milestone 3a as built: Telegram (`telegram.rs`)

- **Your own bot.** Settings → Telegram takes a token from @BotFather. AgentDeck checks it with `getMe`, keeps it in the keychain (`secrets::set_named("telegram-bot-token")`), and long-polls `getUpdates` (25 s) on its own thread. Nothing listens on your Mac and there's no server in between. Errors never include the request URL (it carries the token).
- **Pairing.** "Pair a chat" shows a 6-digit code for 10 minutes; `/pair <code>` from a private chat pairs it. Five wrong tries end the code. Groups never pair (everyone in a group could give your bots work). Unpaired chats are told once that the bot is private and are otherwise ignored. Paired chats are kept in `meta` (`telegram.chats`); the update offset in `telegram.offset`.
- **Tasks.** A message in a paired chat is a task (`source = "telegram:<chat>:<message>"`, shown to the agent as "you (sent from Telegram)") for the chat's bot: the one picked with `/use`, a button, or the only bot there is; "Name: …" or "@Name …" picks one for that message. `/bots`, `/status`, `/cancel`, `/unpair`, `/help`.
- **Results.** A task sent from Telegram reports to that chat as a reply to the message. Others report to all paired chats as chosen in Settings: nothing else, scheduled tasks (default) or every bot task. Long answers are split at line breaks under Telegram's 4096 characters.
- **Needs you.** The runner notes `bot_needs_you` for held requests and now for questions too. Telegram sends held requests with Allow / Deny, Claude Code plans (`ExitPlanMode`) with Approve plan / Keep planning, and Claude Code questions (`AskUserQuestion`) one message per question with its choices (multi-select: pick, then Done; Skip; or reply to the message with your own words). Buttons carry a short token (button data holds 64 bytes) mapped to the request in memory. When a request is answered anywhere (`permission_decision`, `question_answered`), its Telegram messages say so and lose their buttons. Other agents' questions: a note to answer in AgentDeck.
- **Not yet:** voice notes, photos and files; approvals from your own (non-bot) sessions; the phone web page.

## Milestone 3b as built: connections, triggers, ready-made bots

### Connections (`connections.rs`, `mac.rs`)

- A connection is an MCP server: a command run on the Mac (stdio), an `https://` address (streamable HTTP, headers), or AgentDeck's own **Mac** tools. Presets: Browser (`npx -y @playwright/mcp@latest`; every step waits for you unless you let it act alone, since it could send what it read anywhere), GitHub (`https://api.githubcopilot.com/mcp/` with `Authorization: Bearer <token>`), Notion (`npx -y @notionhq/notion-mcp-server`, `NOTION_TOKEN`), Mac. Your own: any command with environment variables, or any address with headers.
- Stored in `meta` (`connections`; per bot `bot.connections.<bot id>`). Secret values live in the keychain (`connection:<id>:<key>`), never in SQLite; the list only says whether one is saved. Removing a connection or one of its secret fields removes it from the keychain.
- A bot session's agent gets its connections when it starts (`mcp::Launch`): Claude Code in its `--mcp-config` file (0600, as the agentdeck server already was); Codex with `-c mcp_servers.<name>.…`, secrets in Codex's environment under names it keeps from its shell (`env_vars` for variables named *KEY*/*SECRET*/*TOKEN*, `env_http_headers` → `AGENTDECK_CONNECTION_<i>_<j>_TOKEN` for headers); ACP agents in `session/new`. Changing a bot's connections stops its idle agents so its next task starts them with the new set.
- Approvals while a bot works on a task: a connection's tool that reads (by the first word of its name: get, list, search, read, fetch, find, query, view, snapshot, navigate…) is allowed; one that changes something is held ("it changes something in GitHub (merge_pull_request)") and reaches you in AgentDeck and on Telegram, unless the connection may act on its own (Codex: `default_tools_approval_mode = "approve"` for those).
- **Mac** is the bridge started with `--mac` (`mcp::serve_kit`), calling back into the app, which runs small AppleScript-JavaScript scripts (`osascript -l JavaScript`, input as a JSON argument): `calendar_events`, `calendar_add_event`, `reminders_list`, `reminders_add`, `mail_recent`, `mail_draft` (opens a draft, never sends), `notes_add`. macOS asks once per app (Automation); the app has `NSAppleEventsUsageDescription` and the `com.apple.security.automation.apple-events` entitlement. Repeating events show only where they start.
- `notify_me` (agentdeck server, every bot on an agent that takes MCP): a message to every paired Telegram chat and a notification on the Mac.

### Triggers (`triggers.rs`)

- Per bot, in `meta` (`triggers`, state apart in `trigger.state.<id>`): **page** (every N ≥ 5 minutes; fires with the lines that are new and gone, scripts and styles dropped), **folder** (new files at the top level, every minute), **github** (open issues and pull requests with the label, default `agentdeck`, every 2 minutes; only when the label was added by one of the people you list, or else by the repository's owner; the GitHub connection's token when there is one), **hook** (a POST to `http://127.0.0.1:<47810–47814>/hooks/<secret>`; requests with an `Origin` header, i.e. from web pages, are refused).
- The first look only notes what's there. The task is your text, then what's new, marked as information and not instructions. `source = "trigger:<id>"`; Telegram reports them like scheduled tasks.

### Ready-made bots

- The New bot dialog starts from Researcher, Inbox assistant, Day planner, Social media writer, Watcher or Developer: name, job, instructions, the matching connections you have, and the next step to set up (a schedule or a trigger).

## Orchestration across models (0.7.0)

- **Per-task model and effort.** `delegate_task` takes optional `model` and `effort`. A bot lists the other models a manager may pick for it (bot options, `meta` `bot.options.<id>`), and `list_bots` shows them as `model_choices`. Anything outside those choices is refused with the list; effort only for Claude Code and Codex bots. The task keeps its model and effort (`bot_tasks.model`, `.effort`, schema 10); its session switches to them (`agents.set_model` / `set_effort` on a live agent) and back to the bot's own for the next task.
- **A worktree per task.** With the bot option on and a task in a project that's a git repository, the task gets `git worktree add -b agentdeck/<bot>-<task>` next to the project (`<project>.worktrees/…`) and a fresh session there. Its header says which branch it's on and not to switch, merge or push unless asked; its result ends with the branch and folder; `task_status`/`wait_for_tasks` report the branch. So bots work on one repository at the same time. The branch is merged by you (the session's worktree controls) or by a bot you ask. The task's session maps to the bot through `bot_tasks.session_id` (only worktree tasks, so a let-go place session stays let go).
- **Usage.** From Claude Code's `result`: tokens (input + cache read + cache creation, output) per turn and the cost at API prices (the session's running `total_cost_usd`, so a turn's is the difference; a new process counts from zero). Kept per task (`tokens_in`, `tokens_out`, `cost_usd`); the bot page sums the last 24 hours, 7 and 30 days next to the agent's plan limits. Other agents don't report it yet.
