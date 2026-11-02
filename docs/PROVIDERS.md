# Provider integration routes

Checked against official documentation and the installed CLIs on 2026-11-02. All four CLIs ship frequently; re-check before relying on a row.

## Claude Code — route: the user's unmodified `claude` CLI, driven over stream-json

**Interface.** `claude -p` with `--input-format stream-json --output-format stream-json --verbose` gives newline-delimited JSON events for the whole agent loop: a `system/init` event (model, tools, MCP servers, and a `capabilities` array for feature detection), assistant and tool messages, `system/api_retry`, and a final `result` with session ID and cost. Sessions continue with `--resume <session_id>`. SIGINT ends a turn cleanly; SIGTERM leaves it unfinished.

**Permissions.** In a non-interactive run, a call that needs approval is denied unless something answers it. AgentDeck answers through `--permission-prompt-tool` (an MCP tool AgentDeck hosts locally), which maps 1:1 onto the Permission card: the request is held until the user decides, and the decision goes back to that specific request.

**Authentication — the constraint that shapes the design.** Anthropic's legal and compliance page says subscription OAuth is for Claude Code and Anthropic's own apps, that third-party developers may not offer claude.ai login inside their own applications or collect, store or intermediate Claude credentials, and that products built on the Agent SDK should use API keys. It also says this does not prevent an end user from signing in to the *unmodified* Claude Code binary with their own subscription.

So AgentDeck will:

- launch the `claude` binary the user installed, unmodified, as the user
- never show its own Claude login screen, never read or store Claude tokens, never pass `CLAUDE_CODE_OAUTH_TOKEN`
- send users to `claude auth login` in their own terminal when the CLI reports it isn't signed in (the crash card shows Claude Code's own error text)
- show sign-in status only as reported by the CLI itself
- not embed the TypeScript Agent SDK in the app for now, because the SDK path is the one Anthropic explicitly ties to API keys for products

**Open question for you:** if AgentDeck is ever sold to other people, read the Commercial Terms conditions on running Claude Code inside a product (no modified binary, no paying for or intermediating end users' usage) before launch.

## Codex — route: Codex App Server (JSON-RPC over stdio)

OpenAI describes the App Server as the first-class integration it maintains for clients such as the VS Code extension and the Codex desktop app. It runs as a long-lived process speaking JSON-RPC over stdio (JSONL framing). Work is modeled as **threads** (durable, resumable, forkable), **turns** (one unit of agent work) and **items** (typed messages, tool runs, diffs, approval requests) with started → delta → completed lifecycles. When Codex needs permission, the server sends a request and the turn pauses until the client answers. Sign-in, including Sign in with ChatGPT, is handled by Codex itself.

This maps directly onto the normalized event stream and avoids parsing terminal output.

**What AgentDeck does (milestone 4, tested with codex-cli 0.166.0).** Starts `codex app-server` from the user's own install, sends `initialize` / `initialized`, then `thread/start` (or `thread/resume` with the stored thread id) with `approvalPolicy: "untrusted"` and `sandbox: "workspace-write"`, so anything outside Codex's read-only command set asks first. Each prompt is a `turn/start`; Stop sends `turn/interrupt`. Approval requests (`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, and the older `execCommandApproval` / `applyPatchApproval`) are held until the user answers that exact request. "Allow for session" is shown only when Codex lists `acceptForSession` for that request; the persistent exec-policy amendment Codex can offer is never sent. Codex numbers its requests from 0 in each process, so AgentDeck prefixes them per process to keep ids unique across restarts. Elicitations and dynamic client tools are refused with a JSON-RPC error rather than silently ignored. AgentDeck never reads `~/.codex/auth.json`; sign-in stays with Codex.

## Gemini CLI and Grok Build — route: Agent Client Protocol (ACP) over stdio

Both CLIs speak ACP, the JSON-RPC protocol Zed and other editors use to host agents: `gemini --acp` and `grok agent stdio`. AgentDeck sends `initialize`, then `session/new`, or `session/load` with the stored id when the agent says it can (`agentCapabilities.loadSession`), and one `session/prompt` per message. It advertises no file-system or terminal capabilities, so the agent reads and writes files itself under its own rules. `session/request_permission` is answered with one of the options the agent offered: Allow → `allow_once`, the session button → the agent's own `allow_always` option under the agent's own name (Gemini calls it "Allow for this session"), Deny → `reject_once`. Stop sends `session/cancel` and answers open requests as cancelled. Plan mode uses the agent's own `plan` mode via `session/set_mode` when it has one (Gemini does), otherwise AgentDeck refuses every request while planning.

Gemini CLI 0.66 answers `session/load` while it is still replaying the old conversation, so AgentDeck waits for the replay to go quiet before sending the next message, and doesn't store the replayed updates a second time.

Grok Build is implemented from xAI's documentation but untested: its installer couldn't be reached from AgentDeck's test environment.

## Custom models: DeepSeek, Kimi, local models, anything compatible

A **model profile** (Settings → Models) is a name, an address, a model name and an optional API key. The key goes to the OS keychain (macOS Keychain) and is never written to SQLite, events or logs, never sent back to the window, and masked if a server echoes it in an error. Plain `http://` is accepted only for this computer (localhost, 127.0.0.1); anything else must be `https://`. A profile runs one of two ways:

- **Anthropic-compatible → inside your Claude Code.** AgentDeck starts the same unmodified `claude` binary with `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` (from the keychain), `ANTHROPIC_MODEL` and every model tier (`ANTHROPIC_DEFAULT_OPUS/SONNET/FABLE/HAIKU_MODEL`, `ANTHROPIC_SMALL_FAST_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`) set for that session only, and removes `ANTHROPIC_API_KEY` from its environment. Everything Claude Code sessions do in AgentDeck works: permission cards, plan mode, hand-off, relay. Endpoints: DeepSeek `https://api.deepseek.com/anthropic`, Kimi `https://api.moonshot.ai/anthropic`, Ollama `http://localhost:11434`. If your `~/.claude/settings.json` has its own `env` block with these variables, Claude Code lets that win.
- **OpenAI-compatible → AgentDeck's own agent** (`src-tauri/src/api.rs`). Streams `POST {address}/chat/completions` with six function tools: `list_files`, `read_file`, `search` (run freely), `write_file`, `edit_file`, `run_command` (each asks you first, with a diff for file changes; "allow for this session" covers all edits, or that exact command). Paths are resolved inside the project folder and symlinks out are refused. Commands run in your login shell in the project folder with a 120 s limit. Plan mode offers only the read tools. The conversation is rebuilt from the stored events, so it continues after a restart. Up to 40 tool steps per message. Works with DeepSeek, Kimi, OpenRouter, Ollama, LM Studio, vLLM and other servers that implement streamed chat completions with tools.

Tested: Claude Code against the Anthropic mock through a profile (`custom_profile_runs_claude_code_on_another_endpoint`), the built-in agent against the chat-completions mock (`api_agent_asks_writes_resumes_and_respects_deny`), both checking that the key never reaches the event store. Model names and endpoints change often; check the provider's current list.

## Capability matrix

| Capability | Claude Code | Codex | Gemini CLI | Grok Build |
|---|---|---|---|---|
| Detect installed CLI and version | **Supported** (found 2.1.316) | **Supported** (found 0.166.0) | **Supported** (found 0.66.0) | **Supported** (looks for `grok`) |
| Create / resume session | **Supported** (`--resume`, tested) | **Supported** (`thread/start`, `thread/resume`, tested across an app restart) | **Supported** (`session/new`, `session/load`, tested) | Implemented, untested |
| Streaming text and tools | **Supported** (stream-json + partial messages) | **Supported** (item deltas) | **Supported** (`session/update` chunks, tool calls) | Implemented, untested |
| Permission requests | **Supported** (allow/deny tested) | **Supported** (accept/decline tested) | **Supported** (allow/reject tested) | Implemented, untested |
| Session-wide allow | **Partial**: only the rules Claude Code suggests | **Partial**: only when Codex offers `acceptForSession` | **Partial**: Gemini's own "Allow for this session" | The agent's own `allow_always` option |
| Plan mode | **Supported**: `--permission-mode plan`, AgentDeck also refuses changes; ExitPlanMode becomes a plan card (tested) | **Supported**: read-only sandbox per turn, approvals refused (tested) | **Supported**: Gemini's own plan mode (tested) | Agent's plan mode if offered, else AgentDeck refuses |
| Interrupt | **Supported** (control request, tested) | **Supported** (`turn/interrupt`) | **Supported** (`session/cancel`) | Implemented |
| File diffs | **Supported** (`structuredPatch`) | **Supported** (`fileChange` diffs) | **Supported** (ACP diff content) | Implemented |
| Model selection | **Supported** (`--model` alias) | **Partial**: Codex config | **Partial**: Gemini config | **Partial**: Grok config |
| Hand off / relay | **Supported** (relay tested Claude ⇄ Codex) | **Supported** | **Supported** | Implemented |

AgentDeck's own agent (custom OpenAI-compatible models): sessions and restart **supported**, streaming **supported**, permission requests **supported** (write/edit/run), session-wide allow **supported** (AgentDeck-side), plan mode **supported** (read tools only), interrupt **supported**, diffs **supported**, model from the profile.

"Tested" means the real CLI ran against a local mock of its model API (`scripts/mock_*.py`) in an automated test, and in the desktop app; see `docs/STATUS.md`.
