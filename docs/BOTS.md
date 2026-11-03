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

Deleting a bot deletes its tasks; its session stays as an ordinary session (renamed "<name> (deleted bot)").

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
- A schedule whose bot is paused still adds its task (it waits in the queue); a schedule never adds a second task while its previous one is still queued or running (it skips that run and says so in the bot's activity).
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

### Tests (milestone 2)

Workspace creation, rename and migration of existing bots; per-place sessions; the unattended rule with workspace and task folder; schedule next-run computation (daily with weekdays, interval, DST change, missed runs), no double-enqueue; the MCP bridge (handshake, token check, each tool, refusal of self-delegation, depth and cycles), and an integration test with the real `claude` and the mock API in which a manager bot delegates a task to a second bot and uses its result.
