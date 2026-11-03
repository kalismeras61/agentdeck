# Bots

A bot is an agent that stays: a name, a job, instructions, an agent and model, a folder to work in, a memory file it keeps, and a queue of tasks it works through one at a time on your Mac, while you do something else. You give it work from AgentDeck (later also on a schedule, from another bot, from your phone or from Telegram), and it reports back.

Bots are built from what AgentDeck already has: each bot owns one ongoing session (any agent: Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity, your own models), so its conversation, diffs, approvals, questions, model and effort pickers all work as in any session.

## Milestones

1. **Bots** (this document's first part): create and edit bots, the task queue (queued → running → done / failed / cancelled; pause, resume, cancel, retry), the memory file, unattended approvals, a Bots section in the sidebar and a bot page.
2. **Schedules and a manager bot**: tasks that repeat ("weekdays at 9:00", "every 6 hours"); a bot can hand tasks to other bots and use their results, through an AgentDeck MCP server the agents call (`list_bots`, `delegate_task`, `task_result`).
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
