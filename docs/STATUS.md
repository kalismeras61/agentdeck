# Status

Updated 2026-11-03. All five milestones are built and tested on Linux with the real `claude`, `codex` and `gemini` CLIs against local mock APIs; milestone 3 was also confirmed on a Mac with a real signed-in Claude Code. Grok Build is implemented but untested (its installer isn't reachable from the test environment).

## Connections and triggers for bots (0.6.0)

| What | How it's checked |
|---|---|
| Connections (stdio command, https address, Mac tools) with secrets only in the keychain; per-bot choice | `secrets_stay_in_the_keychain_and_reach_the_agent_filled_in`, `names_and_addresses_are_checked` |
| Each agent gets them its own way: Claude Code's config file, Codex `-c` with secrets in variables kept from its shell, ACP `session/new` | `connections_reach_each_agent_with_secrets_out_of_the_shell` |
| Reading is allowed, changing waits unless the connection may act alone (Claude Code, Codex, Gemini CLI request shapes) | `reading_is_allowed_changing_waits_unless_it_may_act_alone` |
| Mac tools: listed by the `--mac` bridge only; inputs checked; scripts take input as an argument | `the_mac_bridge_lists_the_mac_tools_only`, `every_tool_has_a_script_a_schema_and_a_kind`, `inputs_are_checked_before_anything_runs`. Not yet run on a Mac |
| Triggers: page changes with what changed, new files once, GitHub labels added by people you allow, hook POSTs from this Mac (web pages refused) | `a_page_fires_when_it_changes_with_what_changed`, `new_files_in_a_folder_fire_once`, `a_github_label_fires_when_someone_you_allow_adds_it`, `a_hook_is_a_post_to_this_mac_with_its_secret` |
| Not yet tried with the real Playwright, GitHub and Notion servers, nor GitHub's API | |

## Telegram for bots (0.6.0)

| What | How it's checked |
|---|---|
| Your own bot token, checked with Telegram, kept in the keychain; long polling, no listening port; errors never show the token | `the_bot_api_is_called_as_telegram_expects_and_errors_never_show_the_token` (a local stand-in for the Bot API), `a_token_is_checked_before_anything_is_saved` |
| Only private chats paired with a one-time code (10 minutes, 5 tries) reach the bots | `only_a_chat_paired_with_the_code_reaches_the_bots` |
| A message is a task for the chosen bot ("Name: …" picks one); its answer comes back as a reply; scheduled results as chosen | `a_message_is_a_task_and_its_answer_comes_back_to_the_chat`, `scheduled_results_go_to_paired_chats_as_you_chose` |
| Risky steps, plans and questions with buttons; answered in AgentDeck, the Telegram message says so | `a_risky_step_is_allowed_or_denied_from_telegram`, `a_question_is_asked_with_its_choices`, `answered_in_agentdeck_the_telegram_message_says_so` |
| Not yet tried against the real api.telegram.org (not reachable from where it was built) | |

## Remote Control from a Claude Code session (0.5.1)

| What | How it's checked |
|---|---|
| `/remote-control` or `/rc` in a Claude Code session: AgentDeck ends its own Claude Code process for it and resumes the same conversation in a terminal with `claude --resume … --remote-control=<title>` (model, effort and plan mode kept), which shows Claude Code's QR code and link; AgentDeck won't start Claude Code for that session meanwhile; when the terminal ends, the messages written there (phone, claude.ai or terminal) are added from Claude Code's conversation file and the session is AgentDeck's again | `remote` tests (2, with a stand-in `claude`); `pty` program-terminal test; `remote-control.test.ts`. Not yet run with a real Claude plan sign-in |
| Not offered for other agents, custom model profiles (Remote Control needs a Claude plan), bots, or while Claude is working | `only_a_claude_code_conversation_on_a_claude_plan_that_is_not_busy` |
| Known gap: if AgentDeck quits while a conversation is in Remote Control, the messages from there aren't added (Claude Code still has them when the conversation continues) | |

## Release: signed, notarized, universal, self-updating (0.2.0)

| What | How it's checked |
|---|---|
| One disk image for Apple Silicon and Intel, signed with the Developer ID, notarized and stapled; `release-mac.command` stops with plain steps when the certificate or notary credentials are missing | Run on the Mac (log in `agentdeck-release.log`) |
| Updates: the app checks the public releases repo at start and every six hours, offers Install and restart above Settings (saying how many working agents a restart stops), verifies the signature and the signed version | Linux desktop run with stand-in answers; Settings → Updates |
| GitHub Actions: tests, then build, sign, notarize and publish a release of the repo on a `v*` tag; `setup-release-secrets.command` fills the secrets with passwords you type | `docs/RELEASING.md` |
| Get started: first run lists each agent (installed, version, Codex sign-in), installs or signs in with the agent's own official command in macOS Terminal, rechecks when you come back | `setup` tests (2); Linux desktop run |
| Kimi Code (Moonshot AI) as an agent: `kimi acp` over the Agent Client Protocol, in every agent picker, Get started (official installer, `kimi login`), models from the agent; GLM (Z.ai) preset for Claude Code | Handshake checked against Kimi Code CLI 2.4.1 (`initialize` answers with ACP v1 and a terminal login); no signed-in run yet |
| Slash commands: "/" in the message box lists the agent's own commands for the session's folder (Claude Code's built-ins, your commands and skills from its `initialize` answer; Gemini CLI's `available_commands_update`; Codex prompts, whose text goes into the message) plus AgentDeck's (/plan, /terminal, /rename, /new, /workflow, /git, /setup); arrows, Enter or Tab, Escape | `commands` tests (2); Linux desktop run |
| Questions: Claude Code's AskUserQuestion and Codex's request_user_input show as a question card (options to pick, one or several, or your own answer; Codex secret answers hidden and not stored); Send answer or Skip, let the agent decide; workflows wait for your answer | `question.test.ts`; `answers_a_question_from_claude_code` (real Claude Code against the stand-in API) |
| Events with overlong text (a huge command line) are shown cut with "…" instead of skipped | `events.test.ts` |
| Workflows run on their own: each stage hands over when its turn ends, review notes go straight back to the builder; approval requests from the agents are answered by AgentDeck except risky ones (deleting, git push/reset, publishing, deploying, credentials, `.env`, sudo, piping downloads into a shell, files outside the project), which wait for you with a note saying why; the built-in "Plan, then build" no longer stops for the plan | `workflows_answer_ordinary_requests_and_leave_risky_ones`; `claude_plans_codex_builds_claude_reviews_until_approved` (real Claude Code and Codex against stand-in APIs) |
| Antigravity approval changes apply at once: from a session, a turn in progress is ended and picked up under the new level, and a turn that skipped actions is redone; other sessions restart with it | `a_session_with_agy_writes_files_reports_skipped_actions_and_continues` |
| Antigravity off by default because of Google's terms; Settings turns it on after you tick that you understand the risk; people who already used it keep it | `antigravity_is_off_unless_turned_on_or_already_used` |

## Git view and worktrees

| Item | Evidence |
|---|---|
| Git view (⌘⇧H) per project: branches with ahead/behind, worktrees with the session working in each, tags, remote branches; the commit graph of all branches (lanes laid out so the main line stays on the left, 300 commits at a time); a commit's files and their diffs in Monaco; uncommitted changes of the project folder or of any worktree | `git` tests (4), `gitGraph.test.ts` (4); `m5/git-view.png` |
| Sessions in their own worktree: "New session in a worktree", the Worktree switch before the first message, or "Work in a new git worktree" for a whole workflow (all stages share it). The worktree goes next to the project as `<project>.worktrees/<name>` on a branch `agentdeck/<name>`; the agent, its terminal and its diffs all work there and the project folder isn't touched | `a_session_worktree_is_made_merged_and_removed`; `m5/worktree-new.png`, `m5/worktree-session.png` |
| Merge from the branch badge: uncommitted work is committed first (named after the session), then `git merge --no-ff` into the project folder's branch; refused while the project folder has uncommitted changes; on a conflict the merge is undone, the files are listed and the agent can be asked to bring the base in and resolve them | `a_conflicting_merge_is_undone_and_reported`; `m5/worktree-merge.png` |
| Remove the worktree (optionally its branch) after a confirmation that says what would be lost; the session goes back to the project folder with a fresh conversation | Linux desktop run |
| Revisions, branch names and paths from the interface are checked before git runs; git runs with fixed arguments, never through a shell; nothing pushes or rewrites history | `checks_what_comes_from_the_interface` |

## Same setup as Terminal

| Item | Evidence |
|---|---|
| AgentDeck reads your login shell's environment once at startup (`$SHELL -l -i`, 6 s limit) and starts every agent with it: PATH from nvm/Homebrew first, plus anything `~/.zshrc` exports (Google Cloud project, proxies, keys). An agent that works in Terminal works the same here; API keys saved in AgentDeck are optional | `shellenv` tests (2), one reading a real bash profile; Linux desktop run picks up `/opt/node22/bin/claude` from the profile |

## Signing in from the conversation

| Item | Evidence |
|---|---|
| When an agent asks you to sign in, the error gets a Sign in button: the agent's own login (`claude auth login`, `codex login`, `gemini`, `grok`) runs in AgentDeck's terminal, links open in your browser, and "I'm signed in, try again" restarts the agent and sends your last message again | `signin.test.ts`; `m5/signin-terminal.png` |
| Gemini CLI's "no longer supported for Gemini Code Assist for individuals" is told apart: signing in again can't fix it, so the options shown are a Gemini API key, a Workspace or Cloud account through Gemini CLI's `/auth`, or Google's Antigravity CLI (`agy`) opened in the terminal for you to use yourself | `m5/signin-gemini-blocked.png` (mock answers with Google's 403) |
| An agent that can't reopen its earlier conversation starts a new one instead of stopping | `acp_gemini_starts_fresh_when_its_old_conversation_is_gone` |

## Antigravity (Google's `agy`)

| Item | Evidence |
|---|---|
| Antigravity sessions through agy's documented headless mode (`--input-format stream-json --output-format stream-json`, one process per session, `--conversation` to continue, `--model`, `--mode plan` for plan mode); its text, tools and file changes show like the other agents', with diffs taken against a `git stash create` snapshot of the project at the start of each turn | `agy` tests (3), one end to end against `scripts/mock_agy.py`; `m5/antigravity-session.png` |
| agy can't ask before acting in headless mode, so sessions run in `accept-edits`: file edits in the project go ahead (review them in Changes); anything else that needs approval is skipped and listed with how to allow it; Stop ends the process and the next message continues the conversation | same test; notice at the start of every Antigravity session |
| Skipped actions get a card with the allow rule agy names (`command(npm test)`); "Allow and send again" adds the ticked rules to agy's own `~/.gemini/antigravity-cli/settings.json` (`permissions.allow`, everything else kept, a broken file left alone) and sends the last message again. Rules that delete, push or change permissions start unticked | `finds_the_rule_to_allow_and_saves_it_in_agys_settings`; `m5/antigravity-skipped.png` |
| Approvals for Antigravity (composer chip, Settings → Providers, or the skipped-actions card): File edits (default), Auto (agy's own `--mode auto`: safe edits and commands go ahead, irreversible ones are skipped) or Everything (`--dangerously-skip-permissions`, only after a second click); running sessions restart with it and keep their conversation; each session says which level it runs with | `starts_agy_with_the_sessions_settings`; `m5/antigravity-approvals.png` |
| Models from `agy models --output-format json`; picking one restarts agy on the same conversation | `m5/antigravity-models.png` |
| Not yet run against the real `agy` (its installer isn't reachable from the test environment); built to the protocol on antigravity.google/docs/cli/headless. Google's terms limit using personal Antigravity sign-ins through other tools; the Settings description says so | — |

## Models: every model each agent offers

| Item | Evidence |
|---|---|
| The model list comes from the agent itself: Claude Code's `initialize` answer, Codex `model/list`, Gemini CLI's `session/new` answer (Grok Build the same way). Lists are asked for once per run with a short-lived process (nothing sent to a model), refreshed by every real session, and saved for the next launch | `models` test; `the_installed_agents_list_their_models` (real Claude Code 2.1.316: 5 models; Codex 0.166: 10) |
| Any other model id can be typed in. A change applies from the next message, without restarting the agent: Claude Code `set_model`, Codex per-turn `model`, ACP `session/set_model` | real Claude Code switched Sonnet → Haiku mid-session; `a_model_picked_while_codex_runs_is_used_from_the_next_turn`; `acp_gemini_lists_its_models_and_uses_the_one_picked` (the mock received gemini-2.5-pro) |
| Default model per agent for new sessions, and any of the four agents as the default agent, in Settings → Workspace defaults | `m5/models-codex.png`, `-claude.png`, `-defaults.png` |

## Agent setup: instructions, skills, commands, subagents

| Item | Evidence |
|---|---|
| A tree (⌘⇧E): Global first, then every project folder, each opening to Instructions, Skills, Commands and Subagents; opened from the sidebar it shows the global files, from a project's book icon that project's | `m5/setup-tree.png` |
| What each agent reads: CLAUDE.md / CLAUDE.local.md / AGENTS.md / GEMINI.md, skills (`.claude/skills`, `.agents/skills`, `.gemini/skills`), commands (`.claude/commands`, `.gemini/commands/*.toml`, Codex prompts), Claude Code subagents; project and global, with names and descriptions from each file | `context` tests (2); `m5/setup-edit.png` |
| Edit in place with ⌘S; a file changed on disk since it was opened isn't overwritten (Reload offered); unsaved edits survive switching files and views; global files warn that every project is affected; linked (symlinked) skills are written where they live | `only_agent_files_can_be_read_or_saved` |
| Only those agent files can be read or written: other paths, `..`, hidden folders and projects not in AgentDeck are refused | same test |
| New skill, command or subagent from a short template, in the place the chosen agent reads | `m5/setup-new-subagent.png` |

## Workflows: plan → build → review

| Item | Evidence |
|---|---|
| Any agent on each stage: Claude Code, Codex, Gemini CLI, Grok Build or any model profile (DeepSeek, Kimi, local); a stage can be skipped except Build | `flow` tests (4) |
| The planner and the reviewer run in plan mode, so AgentDeck refuses their changes; a Claude Code plan presented with ExitPlanMode is taken for the builder instead of waiting for approval | `plan_goes_to_build_then_review_and_notes_go_back_until_approved` |
| The reviewer's notes go back to the builder until it writes APPROVED or the fix limit is reached; "Show me the plan before building" waits for Approve and build / Ask for changes | `gate_waits_for_the_user_and_review_limit_stops`; `m4/workflow-approve.png` |
| End to end with the real CLIs: Claude Code plans, Codex builds, Claude Code asks for one fix, Codex fixes it, Claude Code approves (about 4 s against the mocks) | `claude_plans_codex_builds_claude_reviews_until_approved`; `flow.test.ts` (5) on the recorded run |
| Start from Work with… (header, ⋯ menu, empty session); ready-made workflows and your own in Settings → Workflows; stage bar on every session of the run | `m4/workflow-dialog.png`, `-review.png`, `-settings.png` |

## Footprint (measured 2026-11-02)

| What | Mac (release, Activity Monitor) | Linux (release, private memory) |
|---|---|---|
| AgentDeck itself | 31 MB | 49 MB |
| Interface (WebView) | 207 MB | 146 MB |
| Graphics + networking helpers | 18 MB | 10 MB |
| CPU while idle | 0.1 %, 0 idle wake-ups | 0.4 % |
| Download / installed | 22 MB .dmg / 50 MB app (universal, 0.2.0; Apple Silicon only was 8 MB / 20 MB) | 28 MB binary |
| Claude Code process (per busy session) | — | 188 MB |
| Codex process | — | 159 MB |
| Gemini CLI processes | — | 138 + 206 MB |

Agent processes close after 10 idle minutes and at most 6 stay open (`idle_agents_close_and_the_oldest_makes_room`); the 30 newest sessions load at startup and the rest when opened; long conversations draw 80 runs at a time.

## Import sessions from the agents

| Item | Evidence |
|---|---|
| Finds what Claude Code (`~/.claude/projects`), Codex (`$CODEX_HOME/sessions`) and Gemini CLI (`~/.gemini/tmp/*/chats`) saved; reads conversation files only, never credentials; leaves out the agents' own context, meta lines, sub-agents and tool chatter | `import` tests (2): each format, rewritten Gemini records, files outside the agents' folders refused |
| Claude Code sessions go by the folder Claude Code filed them under (the only folder it resumes them from), not the first `cwd` line, which can come from an earlier conversation or a `cd` | `claude_conversation_goes_to_the_folder_it_was_filed_under` |
| Each session goes into the project for the folder it ran in (added if new); unknown, gone or home-folder sessions go to Chats; dates and messages kept; already-imported or AgentDeck-started sessions shown as such | `m4/import-1_dialog.png`, `-3_gemini.png` |
| The next message continues the agent's own conversation: Claude Code (`--resume`, same transcript file), Codex (same thread id), Gemini CLI (`session/load`, same session id) | Linux desktop runs against the mocks; `m4/import-2_claude_resumed.png`; `attach.test.ts` (imported sessions) |
| Grok Build sessions can't be imported yet (its history format isn't available here) | — |

## Chats, new projects, moving sessions

| Item | Evidence |
|---|---|
| Chat without a project: sessions run in `~/AgentDeck/Chats`, made on first use, listed first in the sidebar | `chats_place_and_moving_sessions`; `m4/chats-1_new_chat.png` |
| New project: AgentDeck makes `~/AgentDeck/<name>` and starts a session there; "Use a folder I have…" still adds an existing folder | `m4/chats-2_new_project.png` |
| Move to…: a session moves to Chats or any project and its history stays. Claude Code carries on the same conversation (AgentDeck copies the transcript to where Claude Code looks for the new folder; the original stays); Codex resumes its thread from the new folder; the other agents start fresh there | `moving_a_claude_conversation_keeps_it_resumable`; `m4/chats-3_moved.png` |
| Rename: double-click the title (header or sidebar), F2 on a sidebar row, or Session actions → Rename…; Enter saves, Escape cancels; renaming doesn't reorder the list | Linux desktop run |
| Claude Code can't find a conversation (started from another folder): the error says so and how to fix it | — |
| ⌘N with no project opens a chat instead of asking for a folder; each project header has + for a new session there | UI |

## Attachments: drop, paste, pick

| Item | Evidence |
|---|---|
| Drop files or folders anywhere on the window (the desktop app gives real paths), paste a screenshot with ⌘V, or pick with the paperclip; up to 20 per message, shown as chips | `m4/attach-1_pasted.png` |
| Images go in as images: inline to Claude Code, by local path to Codex; every file is also listed by path (relative inside the project) so any agent can read it with its own tools and permissions | `dropped_image_and_file_reach_claude_code`, `dropped_image_reaches_codex_as_a_local_image` (real CLIs); `attach` tests (2); `m4/attach-2_sent.png` |
| Pasted images are kept in AgentDeck's data folder, never copied into the project; when the web view doesn't hand a pasted image to the page, the desktop app reads it from the system clipboard | `pasted_images_are_kept_in_the_data_folder` |

On Linux the drop itself couldn't be performed in the headless test display; the paste path and everything after the drop were.

## Custom models (DeepSeek, Kimi, local)

| Item | Evidence |
|---|---|
| Model profiles in Settings → Models with presets (DeepSeek, Kimi, Ollama, LM Studio, OpenRouter, custom); keys in the OS keychain only; "Save and test" | `linux-models-1_presets.png`; `keys_never_appear_in_errors` |
| Anthropic-compatible profiles run inside the user's Claude Code with every model tier set for that session | `custom_profile_runs_claude_code_on_another_endpoint` |
| AgentDeck's own agent for OpenAI-compatible APIs: streamed tool calls, project-confined paths, approvals with diffs, session-wide allows, plan mode, interrupt, history rebuilt after restart | `api` tests (7) and `api_agent_asks_writes_resumes_and_respects_deny`; `api.test.ts` (3); `linux-models-2_api_agent_permission.png`, `-3_api_agent_done.png` |
| Profiles appear in New session, Hand off and Relay; sessions are named after their model | UI |

On Linux the kernel keyring isn't available in the test container, so keys were tested with the in-memory store; the Mac uses its Keychain. An unsigned build can ask once per rebuild for keychain access ("Always Allow").

## Milestone 5: terminal, notifications, startup, packaging

| Item | Evidence |
|---|---|
| Terminal (`src-tauri/src/pty.rs`, `src/components/Terminal.tsx`): your login shell in a project folder AgentDeck already knows, xterm.js, resize, UTF-8 split across reads kept whole, 256 KB scrollback kept so the view reattaches, at most 8 terminals, all closed on quit | `pty` tests (2); Linux desktop run `m4/linux-terminal-1_error.png` |
| Smart terminal: ask any live agent in the project about the selection or the last 80 lines; "Explain this error" appears when output looks like a failure; the agent's own commands listed to type back in (you press Return) | `m4/linux-terminal-2_ask.png`, `-3_sent.png` |
| Notifications for approvals, finished turns and errors, only while AgentDeck isn't focused, following Settings | `store.test.ts` (notifications) |
| Startup: terminal, Review, Gallery and Settings load on demand; main chunk 1.1 MB → 446 KB | `vite build` |
| Packaging: `npm run tauri build` makes the .app and .dmg on a Mac. Not signed or notarized: that needs your Apple Developer ID | Mac build script |

## Working together: plan mode, hand-off, relay

| Item | Evidence |
|---|---|
| Plan mode per session. Claude Code: `--permission-mode plan`, switched live with `set_permission_mode`; Claude Code forwards plan-mode-blocked tools to the permission prompt tool instead of refusing them, so AgentDeck refuses every change itself. Codex: read-only sandbox per turn and approvals refused. Gemini: its own plan mode | `plan_mode_keep_planning_then_approve_then_build`, `codex_plan_mode_declines_without_asking`, `acp_gemini_plan_mode_uses_the_agents_plan_mode` |
| Plan card from ExitPlanMode: Build it here (leaves plan mode), Hand off ▾, Keep planning; plan-file writes and tool loading hidden from the steps | `plan.test.ts` (7) on a recorded session; `m4/linux-plan-1_card.png` |
| Hand off: a linked session for any agent in the same project with the plan as its first task; the source agent is told the plan went elsewhere; both sides link to each other | `hand_off_links_both_sessions`; `m4/linux-plan-3_codex_receives.png`, `-4_source_linked.png` |
| Relay: replies forwarded with attribution and round numbers until DONE, the round limit, a failed turn, or Stop; approvals stay with you; stopped on restart | `relay` tests (4) and `claude_and_codex_talk_until_done` (real Claude Code ⇄ real Codex); `relay.test.ts` |
| Rich view of a relay: both agents as one conversation, or one side; `@codex` / `@claude` / `@gemini` / `@grok` routes a message | `m4/linux-relay-2_both_agents.png`, `-3_this_session.png` |

## Gemini CLI and Grok Build (ACP)

| Item | Evidence |
|---|---|
| ACP manager (`src-tauri/src/acp.rs`): initialize, session/new or session/load, prompt, cancel, set_mode; permission options mapped by kind; request ids unique across restarts; replayed history not stored twice | `acp_gemini_allow_resume_deny` (real Gemini CLI 0.66) |
| Normalizer (`src/lib/acp.ts`): replies, tool calls with status, diffs as changesets, plans, permission cards with the agent's own session option | `acp.test.ts` (3) on a recorded session; `m4/linux-gemini-1_permission.png` |

## Milestone 4: live Codex sessions

| Item | Evidence |
|---|---|
| Codex manager (`src-tauri/src/codex.rs`): runs `codex app-server` over JSON-RPC on stdio; `initialize` → `thread/start` or `thread/resume` → `turn/start`; prompts typed before the thread is ready are queued; Stop sends `turn/interrupt` | `codex_accept_resume_decline` (real codex-cli 0.166.0, mock Responses API) |
| Approvals: command and file-change requests held per request id; ids prefixed per process so they stay unique across restarts; "allow for session" only when Codex lists `acceptForSession`; persistent policy amendments never sent; unsupported server requests answered with an error | same test; `codex.test.ts` |
| Normalizer (`src/lib/codex.ts`): replies, commands with output and exit codes, file changes as changesets, plans as progress cards, approvals with risk, withdrawn requests as expired, failed and interrupted turns | `codex.test.ts` (6), including a real recorded session fixture |
| UI: Codex sessions in "This Mac" from the New session menu or as the default agent in Settings; per-provider model list | Linux desktop run `m4/linux-codex-1_permission.png`, `-2_allowed.png`, `-3_resumed_denied.png` (restart between 2 and 3: history restored, thread resumed, denied command not run) |

```bash
python3 scripts/mock_openai.py 8788 &
(cd src-tauri && AGENTDECK_IT=1 cargo test codex -- --ignored)
```


## Milestone 3: live Claude Code sessions

| Item | Evidence |
|---|---|
| SQLite store (`src-tauri/src/db.rs`): projects, sessions, ordered events per session, duplicate provider events dropped by `uuid`, secrets masked before writing, cascade delete | `cargo test` db tests (7) |
| Restart recovery: sessions that were running or waiting become idle with an `app_restart` event; their unanswered requests show as expired | `restart_recovery_resets_live_sessions`; Linux desktop run `m3/linux-e2e-6_expired.png` |
| Session manager (`src-tauri/src/claude.rs`): starts the user's unmodified `claude` with stream-json in/out and `--permission-prompt-tool stdio`; bounded 16 MB line reader; stderr tail kept for crash reports; GUI-safe PATH; this app's own Claude env vars stripped | `cargo test` unit tests |
| Permission bridge: each `can_use_tool` request is held until answered; the answer goes to that exact `request_id`; unknown or already-answered ids are refused; "allow for session" only sends the rules Claude Code itself suggested | `allow_then_resume_then_deny` |
| Resume: after the process ends (or the app restarts), the next message resumes the same Claude conversation with `--resume` | same test; Linux desktop run `m3/linux-e2e-5_resumed.png` |
| Interrupt ends the turn and keeps the process | `interrupt_stops_a_turn` |
| Concurrent sessions stay isolated (own process, own events, own Claude session id) | `concurrent_sessions_stay_isolated` |
| Normalizer (`src/lib/claude.ts`): text, grouped tools with honest titles, diffs from Claude Code's own `structuredPatch`, TodoWrite as a plan card, permissions with risk and edit preview, API retries, crashes with stderr | `claude.test.ts` (12), including a real recorded session fixture |
| UI: "This Mac" workspace with project folders, live sessions, streaming reply text, model choice, stop turn, end process, delete session with confirmation, errors shown at the message box; "Demo" workspace keeps sample data | `store.test.ts`; Linux desktop run screenshots in `screenshots/m3/` |

End-to-end tests run the real Claude Code CLI (2.1.316) against `scripts/mock_anthropic.py`, a local stand-in for the Anthropic API, so they need no account:

```bash
python3 scripts/mock_anthropic.py 8787 &
(cd src-tauri && AGENTDECK_IT=1 cargo test -- --ignored --test-threads=1)
```

Confirmed on a Mac with a real signed-in Claude Code.


## Screens

| # | Screen | State | Evidence |
|---|---|---|---|
| 1 | Workspace | Done (M1) | `screenshots/web-a_1440.png`, `desktop-linux-1440.png` |
| 2 | All sessions (overview) | Done | `m2/web-overview.png` |
| 3 | Review changes (Monaco diff) | Done | `m2/web-review_split.png`, `m2/web-review_unified.png`, `m2/desktop-linux-review.png` |
| 4 | Permissions | Done | `m2/web-permissions.png`, `m2/desktop-linux-permissions.png` |
| 5 | Output gallery | Done | `m2/web-gallery_*.png` |
| 6 | Terminal mode | Done: real shell, ask an agent about output | `m4/linux-terminal-*.png` |
| 7 | Settings | Done, with real CLI detection in the desktop app | `m2/web-settings.png`, `m2/desktop-linux-settings.png` |

## Milestone 2: what was added and how it's verified

| Item | Evidence |
|---|---|
| Sidebar view navigation with live counts (sessions, changed files, pending permissions in warning color) | Screenshots |
| All sessions: grouped by Needs you / Running / Idle / Completed, last task from the session's own events, real elapsed times computed from event timestamps, error line, Review request / files / Open actions, agent filter | `m2/web-overview.png` |
| Review changes: Monaco bundled locally (no CDN), worker inlined, Dart highlighting, side-by-side / unified / whole file, previous/next change, copy path, per-file and toolbar "viewed" marks with progress, J/K/N/P/V keys, new files forced to unified | `scripts` run below; layout toggle checked across file switches |
| Permissions: queue of every pending request across sessions, focused card with session context, edit preview diff, "Allow for session" only when the request says the provider supports it, decisions list | `store.test.ts` (session-scoped approval routed to the right request) |
| Output gallery: markdown, untrusted markdown, tool logs with ANSI and stripped OSC/CSI sequences, 3,200-line log rendered as a 400-line tail, JSON tree with depth/size caps, progress cards, test summaries, images, changesets, decided permissions, and five malformed events rejected by validation | `events.test.ts` (gallery: exactly 5 rejected) |
| Settings: appearance, providers, workspace defaults (used by New session), notification choices, shortcuts, data and privacy, reset with in-page confirmation, copyable diagnostics without paths or credentials | `store.test.ts` (defaults applied to new sessions) |
| Rust `detect_providers`: PATH plus common install folders, fixed `--version` argument, no shell, closed stdin, 5 s timeout, 512-byte read | `cargo test` (3 tests); desktop run found Claude Code 2.1.316 and codex-cli 0.166.0 |
| Rust `app_info`: version, OS, architecture | Shown in Diagnostics |
| CSP allows the Monaco worker (`worker-src 'self' blob:`) and nothing else new | Monaco renders in the desktop window (`m2/desktop-linux-review.png`) |

## Checks run

- `npm run typecheck`: clean
- `npm run lint`: 0 errors, 8 style warnings (fast-refresh exports, setState in effects)
- `npm test`: 69/69
- `cargo fmt --check`, `cargo clippy`: clean; `cargo test`: 40/40, plus 15 end-to-end tests against the real CLIs and the chat-completions mock (`AGENTDECK_IT=1`)
- `python3 scripts/check_keyboard.py` (dev server running): every view tabbed through, every focused control has a visible focus indicator, no looping animation with reduced motion on. 0 failures.
- Contrast (WCAG): secondary text ≥5.9:1, tertiary ≥4.3:1 on every surface in both themes; white on filled buttons 5.7:1.

## Not done yet

- AgentDeck doesn't browse project folders itself; the Files tab lists files the agent touched
- Signed and notarized build (needs an Apple Developer ID)
- Grok Build tested against the real CLI

## Bots

- **Milestone 1** (docs/BOTS.md §1): persistent agents with a job, instructions, agent/model/effort; a task queue run one at a time (pause, resume, cancel, retry); a memory file; unattended approvals with the workflow's risky/outside holdback (`unattended.rs`); sidebar section and bot page.
- **Milestone 2a, a workspace of its own**: every bot has `~/AgentDeck/Bots/<Name>/` (a hidden project) with `MEMORY.md`; renaming moves it; tasks work in a project or in the workspace, one ongoing session per place, with a place switcher on the bot page; milestone 1 bots are migrated (memory moved, session kept). Deleting a bot keeps its sessions and, if it has any, shows its workspace as an ordinary project; removing a project no longer deletes bots (tasks there fail, its schedules turn off, bots fall back to their workspace).
- **Milestone 2b, schedules**: daily at times on chosen weekdays or every N minutes (≥ 15), local time with `chrono` (DST gaps and repeats handled), a 30 s scheduler, missed runs not made up, no second task while the last is queued or running (the skipped run is shown on the schedule's row), Run now, "Next: 09:00" in the sidebar.
- **Milestone 2c, a manager bot** (`mcp.rs`): AgentDeck's own binary with `--mcp-bridge` is a stdio MCP server named `agentdeck` that relays to the app over a Unix socket (0700/0600, per-session random token kept in memory only). Tools `list_bots`, `list_projects`, `delegate_task`, `wait_for_tasks`, `task_status`; no self-delegation, depth ≤ 3, no cycles (up the chain or through waits), waits end early when the caller's task is cancelled, cancelling a task cancels its queued children. Wired for Claude Code (`--mcp-config` file 0600 + `--allowedTools`), Codex (`-c mcp_servers.agentdeck…`, token through `env_vars`), Gemini CLI / Grok Build / Kimi Code (`mcpServers` in `session/new`/`session/load`); Antigravity and API models can't delegate (the editor says so). The parent task lists its children with status and links; a delegated task shows "from <bot>"; the tool calls read "Delegated to Docs Writer", "Waited for 2 tasks".

| What | Evidence |
|---|---|
| Queue, turn end → result, restart recovery, cancel, pause/resume, memory, workspaces, rename, migration, per-place sessions, unattended rule with workspace + task folder, schedules (weekdays, interval, DST, missed runs, no double-enqueue) | `bots` tests (13), `schedule` tests, `unattended` tests; `bots.test.ts` (17) |
| Two queued tasks in order, in the workspace and a project, with the real `claude` | `queued_tasks_run_in_order_in_the_workspace_and_a_project` |
| MCP bridge: handshake and version negotiation, tools/list, each tool, bad token, app not running, tokens per session, file and socket permissions, self-delegation, depth, cycles, wait results/timeout/early return, cancel cascade, approvals for the agentdeck tools never held | `mcp::tests` (11) |
| A manager bot (real Claude Code 2.1.316) delegates to a second bot, waits, and reports its result; no approval asked; token not in the database | `a_manager_bot_hands_a_task_to_another_bot_and_uses_its_result` |
| Codex 0.166.0 starts the server from the `-c` keys, gets the token through `env_vars`, calls `list_bots` and `delegate_task` without an approval | `a_codex_bot_reaches_the_agentdeck_tools` |
| Gemini CLI 0.66 over ACP starts it from `mcpServers` (bot workspace trusted with `GEMINI_CLI_TRUST_WORKSPACE`), asks before the call, the bot runner allows it | `a_gemini_bot_reaches_the_agentdeck_tools` |
| Grok Build and Kimi Code get the same `mcpServers` entry | Argument/JSON shape only (`each_agent_gets_the_server_in_its_own_way`); not run against the real CLIs |
| Interface: manager with two delegated tasks, delegated task "from <bot>", editor note | Web preview screenshots (demo data) |

Known gaps: after an AgentDeck restart a manager's running task goes back to the queue and starts over, so it may hand the same work out again (the children it already made stay); delegated tasks of a deleted bot keep a dangling parent; Gemini CLI bots working in a project only get the tools if the user trusts that folder in Gemini CLI; ACP waits are capped at 9 minutes; the phone page and Telegram (milestone 3) aren't built; bots' sessions still appear in All sessions, Permissions and Overview.

## Languages

- Interface in 11 languages: en, tr, de, ru, es, fr, pt, zh, ja, ko, ar. English text is the key (`t()`, `tn()` in `src/lib/i18n.ts`); catalogs load on demand.
- `node scripts/i18n-extract.mjs` lists every key into `src/locales/en.json`; `node scripts/i18n-validate.mjs` checks each catalog has every key, the same placeholders and its language's plural forms.
- Arabic is right-to-left: logical spacing utilities mirror each column; the sidebar, conversation and context panel keep their order because macOS keeps the window buttons top-left. Code, paths, diffs and terminals stay left-to-right; agent text takes each paragraph's own direction.
- Translations are machine-made and reviewed by checks only; native speakers are welcome to correct them.

## Known limitations

- Codex and Gemini are tested on Linux; the Mac uses the same code.
- A relay can pass a long reply on; replies over 100 KB are cut, with a note to the receiving agent.
- The demo diffs are reconstructed from their hunks; real sessions will supply full before/after text.
- Bundle is large (Monaco + Shiki). Monaco loads only when Review opens; worth code-splitting further in milestone 5.
- Agent processes stop when AgentDeck quits. History is kept and the conversation resumes on the next message, but nothing runs while the app is closed.
- Secret masking covers well-known key formats only. Tool output can still contain sensitive text you print yourself.

## Next

1. Run Codex, Gemini, plan mode, relay and the terminal on the Mac with real sign-ins
