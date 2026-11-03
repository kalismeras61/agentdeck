<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [Français](README.fr.md) · [Português](README.pt.md) · **简体中文** · [日本語](README.ja.md) · [한국어](README.ko.md) · [العربية](README.ar.md)

_这是翻译版本，以[英文 README](../../README.md) 为准。_

# AgentDeck

**一个 Mac 工作区，容纳你已在使用的所有 AI 编程智能体。**

让 Claude Code、Codex、Gemini CLI、Kimi Code 和你自己的模型并排运行。以清晰、结构化的形式阅读它们的输出，审查每一处 diff，在同一个地方回答它们的问题和审批请求，还能让它们互相规划、实现和审查彼此的工作。

用你自己的 CLI，用你自己的账号，无需 API 密钥。

[下载 Mac 版](https://github.com/kalismeras61/agentdeck/releases/latest) · [更新内容](https://github.com/kalismeras61/agentdeck/releases) · [工作原理](#工作原理) · [从源码构建](#从源码构建)

![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Built with Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck：一个 Claude Code 会话正在请求运行命令，右侧是它所做的 diff](../screenshots/readme/workspace.png)

</div>

## 为什么选择 AgentDeck

编程智能体在终端里很好用，直到你在五个项目里同时跑三个。审批请求被刷走，diff 埋在日志里，想让一个智能体检查另一个的工作，只能靠复制粘贴。

AgentDeck 让智能体保持原样，只是给它们一个像样的工作区：

- **运行的是真正的 CLI。** `claude`、`codex`、`gemini` 从你自己的安装启动，PATH、配置和登录状态都与终端中完全相同。在终端里能用的，在这里也能用。
- **从不碰你的凭据。** 每个智能体都通过自己的流程登录。AgentDeck 不读取、不存储、也不发送任何令牌；你为自己的模型添加的 API 密钥保存在 macOS 钥匙串中。
- **一切都留在你的 Mac 上。** 会话保存在本地 SQLite 数据库中。无需账号，没有遥测，没有服务器。

## 你可以做什么

### 在任意项目中使用任意智能体

每个项目就是一个文件夹；每个会话就是一个智能体在其中工作。在发送第一条消息前可以切换智能体，可以从每个智能体提供的所有模型中选择，还能在对话中途切换模型。你可以不选项目直接开始聊天，在项目之间移动会话、重命名会话，或者导入 Claude Code、Codex 和 Gemini CLI 已经保存在你 Mac 上的会话；它们会接着原来的对话继续。

你还可以选择智能体思考的深度。Claude Code 和 Codex 在模型旁边有一个**推理强度**设置（从低到最大，只显示该模型支持的级别）。菜单会显示智能体当前使用的推理强度；在 Claude Code 工作时修改会立即生效，而在设置中可以指定新会话默认使用的推理强度。

### 像审查 pull request 一样审查改动

智能体改动的每个文件都会逐一列出，支持并排或统一 diff 视图，可用键盘导航，并能标记为“已查看”。不会替你提交或推送任何内容。

智能体截取或查看的截图和图片（模拟器截图、渲染出的页面、它读取的图片）会直接显示在对话和“预览”标签页中；点击即可查看原图。

![并排审查智能体的改动](../screenshots/readme/review.png)

### 批准、拒绝，或让它来问你

需要你确认的命令和编辑会以卡片形式出现，说明将运行什么、在哪里运行、会影响什么以及风险有多大。规划模式让智能体在做任何改动之前先阅读代码并提出方案。当智能体有问题要问时（Claude Code 的 AskUserQuestion、Codex 的 request_user_input），你会看到一个真正的表单：选择一个选项、多个选项，或者自己填写。

![一个智能体在提两个问题，一个单选，一个多选](../screenshots/readme/question.png)

### 让智能体协同工作

- **工作流：规划 → 实现 → 审查。** 每个阶段都可以指定任意智能体，例如 Claude Code 负责规划，Codex 负责实现，Claude Code 负责审查。每个阶段一完成就立即移交，审查意见直接返回给实现者，直到审查者写下 APPROVED 为止。普通请求会自动替智能体批准；有风险的请求（删除、推送、部署、凭据，以及任何超出项目范围的操作）仍然会等你决定。
- **移交。** 一键把任何规划或回复发送给另一个智能体。
- **接力。** 两个智能体相互对话，直到其中一个说 DONE。

![一个“规划、实现、审查”工作流：Codex 负责实现，Claude Code 审查并批准](../screenshots/readme/workflow.png)

### 用 git 工作树隔离工作

在独立的工作树和分支中启动会话，这样智能体就不会动到你的工作副本。在 Git 视图中查看分支图，然后把工作树合并回去（如果两边改了相同的行，会给出明确警告），或者直接丢弃。

![将智能体的工作树合并回 master](../screenshots/readme/worktree.png)

### 所有命令触手可及

在消息框中输入 `/`，即可看到智能体在该文件夹中可用的命令（Claude Code 的内置命令、你的自定义命令和技能、Gemini CLI 的命令、你的 Codex 提示词），以及 AgentDeck 自己的命令（`/plan`、`/terminal`、`/workflow`、`/git` …）。

![斜杠命令菜单](../screenshots/readme/slash.png)

### 其他功能

- **智能体设置。** 在同一棵树中编辑 `CLAUDE.md`、`AGENTS.md`、`GEMINI.md`、技能、命令和子智能体，包括全局和单个项目的配置。
- **智能终端。** 在项目文件夹中打开你自己的 shell。可以就选中的内容或最后 80 行向任意智能体提问，还有一键“解释这个错误”。
- **附加任何内容。** 拖入文件或文件夹、粘贴截图，或者选择文件。图片会以图片的形式发送给智能体。
- **你的模型。** DeepSeek、Kimi、GLM (Z.ai)、Ollama、LM Studio、OpenRouter，或任何兼容 OpenAI 或 Anthropic 的端点，同样支持审批、规划模式和工作流。
- **在登录失效处直接登录。** 当智能体需要登录时，点一下按钮就会在内置终端中运行它自己的登录流程，然后重新发送你的消息。
- **自动更新。** 新版本都经过签名和公证，每次更新在安装前都会用内置于应用中的密钥进行校验。

### 支持你的语言

整个界面提供 **English、Türkçe、Deutsch、Русский、Español、Français、Português、简体中文、日本語、한국어 和 العربية** 版本，阿拉伯语使用从右到左的布局。AgentDeck 会跟随你 Mac 的语言，你也可以在 设置 → 外观 中自行选择。智能体的回复、代码、命令和文件名永远不会被翻译，智能体会用你写给它的语言来回答。

![土耳其语和阿拉伯语（从右到左）界面的 AgentDeck](../screenshots/readme/languages.png)

## 轻量，不拖累你的 Mac

AgentDeck 是一款原生 Tauri 应用：一个小巧的 Rust 内核加上 macOS 自带的 WebKit，没有捆绑浏览器引擎。

| | 在 Mac 上实测（发布版构建，活动监视器） |
|---|---|
| 下载 | 22 MB 磁盘映像（Apple Silicon 与 Intel 合为一个应用） |
| 安装后 | 50 MB |
| 内存（AgentDeck 自身） | 约 31 MB 应用 + 约 207 MB 界面 + 约 18 MB 系统辅助进程 ≈ **256 MB** |
| 空闲时 CPU | **0.1 %**，空闲时无唤醒 |
| 启动 | 界面加载一个 446 KB 的主包；审查、终端、展示区和设置在打开时才加载 |

智能体作为独立进程运行，占用各自的内存，工作时每个大约 140–210 MB（在 Linux 上实测：Claude Code 约 188 MB，Codex 约 159 MB）。为了控制占用，空闲 10 分钟的智能体会被关闭（你再次发消息时对话会继续），同时最多保持 6 个处于打开状态，较长的历史记录每次加载 80 轮。

## 支持的智能体

| 智能体 | AgentDeck 如何与之通信 | 登录方式 |
|---|---|---|
| **Claude Code** | `claude`，使用 stream-json 及其权限提示工具 | Claude Pro、Max、Team 或 Enterprise，或 Anthropic Console |
| **Codex** | `codex app-server`，基于 stdio 的 JSON-RPC | ChatGPT 套餐或 OpenAI API 密钥 |
| **Gemini CLI** | Agent Client Protocol | Google Workspace 或 Cloud 账号，或 Gemini API 密钥 |
| **Grok Build** | Agent Client Protocol | xAI 账号或 API 密钥 |
| **Kimi Code** | `kimi acp`，Agent Client Protocol | Kimi Code 套餐或 Moonshot AI API 密钥 |
| **Antigravity** *（实验性，默认关闭）* | `agy` 无头模式 stream-json | Google 账号；请参阅设置中关于 Google 条款的说明 |
| **你的模型** | AgentDeck 内置智能体，或对兼容 Anthropic 的 API 在 Claude Code 中运行 | 存放在你钥匙串中的 API 密钥 |

至少安装一个。首次运行界面会显示你的 Mac 上已有哪些智能体，并在终端中用各智能体的官方命令安装或登录其余的智能体，你可以清楚地看到它在做什么。

## 安装

1. 从 [Releases](https://github.com/kalismeras61/agentdeck/releases/latest) 下载最新的 `AgentDeck-<version>.dmg`。
2. 将 AgentDeck 拖到“应用程序”文件夹并打开。它已签名并经过公证，双击即可打开。
3. 按照**开始使用**设置一个智能体，然后打开一个项目文件夹。

需要 macOS 12 或更高版本，支持 Apple Silicon 和 Intel。

## 工作原理

```
┌──────────────────────── AgentDeck (Tauri app) ────────────────────────┐
│  React UI: conversation, diffs, approvals, questions, workflows       │
│                │  validated, provider-neutral events (Zod)            │
│  Rust core: sessions · SQLite · git · PTY · workflow engine           │
└──────┬──────────────┬──────────────┬──────────────┬───────────────────┘
       │ stream-json  │ JSON-RPC     │ ACP          │ OpenAI / Anthropic API
       ▼              ▼              ▼              ▼
   claude          codex app-server  gemini, grok,  your models
                                     kimi
   (your install, your sign-in, your folder)
```

- 每个智能体都作为子进程在会话所在的文件夹中运行，使用你登录 shell 的环境。
- 它们的输出会原样保存，然后被规范化为统一的事件流（消息、工具调用、diff、审批、问题、规划），并在渲染任何内容之前进行校验。任何格式错误的内容都会显示为明确的错误，绝不会被执行。
- 审批只能由你来回答，或者由工作流替普通请求作答；任何危险操作都不会被自动批准。
- 前端没有 shell 访问权限：它只能调用 AgentDeck 自己的一组受限命令。

## 从源码构建

环境要求：Node 22+、Rust 1.77+，以及 Xcode 命令行工具。

```bash
npm install

npm run dev                 # web preview with labeled demo data, http://localhost:1420
npm run tauri dev           # desktop app

# Checks
npm run typecheck && npm run lint && npm test
(cd src-tauri && cargo fmt --check && cargo clippy && cargo test)

# End to end with the real CLIs against local stand-in APIs (no accounts needed)
python3 scripts/mock_anthropic.py 8787 & python3 scripts/mock_openai.py 8788 & python3 scripts/mock_gemini.py 8789 &
(cd src-tauri && AGENTDECK_IT=1 cargo test -- --ignored --test-threads=1)

npm run tauri build         # .app / .dmg
```

发布流程（签名、公证、通用二进制、自动更新）见 [docs/RELEASING.md](../RELEASING.md)；已完成的部分以及各部分如何验证见 [docs/STATUS.md](../STATUS.md)；智能体协议见 [docs/PROVIDERS.md](../PROVIDERS.md)。

<details>
<summary><b>快捷键</b></summary>

| 操作 | 按键 |
|---|---|
| 新建会话 | ⌘N |
| 搜索会话 | ⌘K |
| 显示/隐藏边栏 / 上下文面板 | ⌘B / ⌘. |
| 上下文标签页 | ⌘1 … ⌘4 |
| 上一个 / 下一个会话 | ⌥↑ / ⌥↓ |
| 聚焦消息框 | ⌘L |
| 命令 | 在消息开头输入 / |
| 富文本视图 / 终端视图 | ⌘⇧R / ⌘⇧T |
| 所有会话 / 审查 / 权限 / Git / 智能体设置 / 展示区 | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| 设置 | ⌘, |
| 重命名会话 | 双击标题，或在边栏中按 F2 |
| 审查：下一个/上一个文件、改动、标记为已查看 | J / K、N / P、V |
| 发送 / 换行 | ↵ / ⇧↵ |

权限请求特意没有设置快捷键。

</details>

<details>
<summary><b>项目结构</b></summary>

```
src/                     React app
  lib/events.ts          normalized event schema and validation
  lib/claude.ts, codex.ts, acp.ts, api.ts, normalize.ts   agent logs → events
  components/            sidebar, conversation, composer, terminal, panels
  components/blocks/     renderers: markdown, code, diff, tools, approvals, questions, plans, workflows
  views/                 review, permissions, git, agent setup, settings, get started
  locales/               interface translations (English text is the key)
src-tauri/src/           Rust core
  claude.rs, codex.rs, acp.rs, agy.rs, api.rs   one manager per agent protocol
  flow.rs, relay.rs      workflows and relays
  git.rs, pty.rs, db.rs, import.rs, context.rs, commands.rs, updates.rs
scripts/                 stand-in APIs for tests, release scripts
docs/                    status, providers, releasing, screenshots
```

技术栈：Tauri 2 (Rust)、React 19、TypeScript (strict)、Vite、Tailwind CSS 4、Radix UI、Zustand、Zod、Shiki、Monaco、xterm.js、SQLite。

</details>

## 参与贡献

欢迎提交 issue 和 pull request。在发起 PR 之前请先运行上面的检查，并保持界面文字简短、平实。

**翻译。** 界面翻译借助 AI 完成并经过测试检查，但尚未经过母语人士审校，因此非常欢迎指正：编辑 `src/locales/<language>.json` 并运行 `node scripts/i18n-validate.mjs`。新增的界面文字需通过 `t()`；详见 [docs/I18N.md](../I18N.md)。

AgentDeck 以 AGPL 许可提供，同时另行提供商业许可。为了保持这种可能性，贡献者需要在首次提交 pull request 时签署贡献者许可协议。

## 许可证

AgentDeck 是自由软件，采用 [GNU Affero 通用公共许可证 v3.0](../../LICENSE)。你可以使用、研究、修改和分享它。如果你通过网络向他人提供修改后的版本，就必须按照同一许可证向他们提供其源代码。

如需不受 AGPL 义务约束的商业许可（例如将 AgentDeck 集成到闭源产品中发布），请通过本仓库与我们联系。

Claude Code、Codex、Gemini CLI、Grok Build、Kimi Code 和 Antigravity 是其各自所有者的产品。AgentDeck 与它们没有关联，也未获得它们的认可；它运行的是你自己安装的副本。
