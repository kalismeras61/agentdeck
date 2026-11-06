<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [Français](README.fr.md) · [Português](README.pt.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · **한국어** · [العربية](README.ar.md)

_이 문서는 번역본이며, 기준이 되는 원문은 [영어 README](../../README.md)입니다._

# AgentDeck

**이미 쓰고 있는 모든 AI 코딩 에이전트를 위한 하나의 Mac 작업 공간.**

Claude Code, Codex, Gemini CLI, Kimi Code, 그리고 직접 연결한 모델을 나란히 실행하세요. 에이전트의 작업을 깔끔하게 구조화된 출력으로 읽고, 모든 diff를 검토하고, 질문과 승인 요청에 한곳에서 답하고, 에이전트끼리 서로의 작업을 계획하고 구현하고 검토하게 하세요.

내 CLI, 내 계정 그대로. API 키는 필요 없습니다.

[Mac용 다운로드](https://github.com/kalismeras61/agentdeck/releases/latest) · [새로운 기능](https://github.com/kalismeras61/agentdeck/releases) · [작동 방식](#작동-방식) · [소스에서 빌드](#소스에서-빌드)

![라이선스: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Tauri 2로 제작](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck: 명령 실행 승인을 요청하는 Claude Code 세션과 오른쪽에 표시된 변경 diff](../screenshots/readme/workspace.png)

</div>

## 왜 AgentDeck인가

코딩 에이전트는 터미널에서도 훌륭합니다. 다섯 개 프로젝트에서 세 개를 동시에 돌리기 전까지는요. 승인 요청은 스크롤 너머로 사라지고, diff는 로그 속에 묻히고, 한 에이전트의 작업을 다른 에이전트에게 확인시키려면 복사해서 붙여넣어야 합니다.

AgentDeck은 에이전트를 있는 그대로 두고, 제대로 된 작업 공간을 제공합니다.

- **실제 CLI를 실행합니다.** `claude`, `codex`, `gemini`는 직접 설치한 그대로, 터미널과 같은 PATH, 설정, 로그인 상태로 시작됩니다. 터미널에서 되면 여기서도 됩니다.
- **자격 증명에 절대 손대지 않습니다.** 각 에이전트는 자체 절차로 로그인합니다. AgentDeck은 토큰을 읽지도, 저장하지도, 전송하지도 않습니다. 직접 연결한 모델용으로 추가한 API 키는 macOS 키체인에 보관됩니다.
- **모든 것이 내 Mac에 남습니다.** 세션은 로컬 SQLite 데이터베이스에 저장됩니다. 계정도, 텔레메트리도, 서버도 없습니다.

## 할 수 있는 일

### 어떤 프로젝트에서든, 어떤 에이전트와도

모든 프로젝트는 폴더이고, 모든 세션은 그 폴더에서 일하는 하나의 에이전트입니다. 첫 메시지를 보내기 전에 에이전트를 바꾸고, 각 에이전트가 제공하는 모든 모델 중에서 고르고, 대화 도중에도 모델을 바꿀 수 있습니다. 프로젝트 없이 채팅을 시작하거나, 세션을 다른 프로젝트로 옮기거나, 이름을 바꾸거나, Claude Code, Codex, Gemini CLI가 이미 Mac에 저장해 둔 세션을 가져올 수 있습니다. 가져온 세션은 같은 대화를 그대로 이어갑니다.

에이전트가 얼마나 깊이 생각할지도 정할 수 있습니다. Claude Code와 Codex에는 모델 옆에 **추론 강도** 설정이 있습니다(낮음부터 최대까지, 해당 모델이 지원하는 단계만 표시). 메뉴에는 에이전트가 지금 사용 중인 추론 강도가 표시되고, Claude Code가 작업하는 중에도 변경 사항이 즉시 적용되며, 새 세션의 기본 추론 강도는 설정에서 정합니다.

### 풀 리퀘스트를 검토하듯 변경 사항 검토

에이전트가 건드린 모든 파일이 나란히 보기 또는 통합 diff로 파일별로 표시되며, 키보드 탐색과 "확인함" 표시를 지원합니다. 커밋이나 푸시를 대신 해 주는 일은 없습니다.

에이전트가 찍거나 살펴본 스크린샷과 이미지(시뮬레이터 스크린샷, 렌더링된 페이지, 에이전트가 읽은 이미지)는 대화와 미리보기 탭에 바로 나타나며, 클릭하면 원래 크기로 볼 수 있습니다.

![에이전트의 변경 사항을 나란히 검토하는 화면](../screenshots/readme/review.png)

### 승인, 거부, 또는 질문하게 두기

승인이 필요한 명령과 편집은 카드로 나타나, 무엇이 어디서 실행되는지, 무엇에 영향을 주는지, 얼마나 위험한지 알려 줍니다. 계획 모드에서는 에이전트가 무엇이든 변경하기 전에 먼저 읽고 제안하게 할 수 있습니다. 에이전트에게 질문이 있으면(Claude Code의 AskUserQuestion, Codex의 request_user_input) 제대로 된 입력 양식이 나타납니다. 옵션을 하나 또는 여러 개 고르거나, 직접 답을 쓰면 됩니다.

![에이전트가 두 가지 질문을 하는 화면: 하나는 단일 선택, 하나는 복수 선택](../screenshots/readme/question.png)

### 에이전트끼리 협업하게 하기

- **워크플로: 계획 → 구현 → 검토.** 단계마다 원하는 에이전트를 배치하세요. 예를 들어 Claude Code가 계획하고, Codex가 구현하고, Claude Code가 검토합니다. 각 단계는 끝나는 즉시 다음 단계로 넘어가고, 검토 의견은 곧바로 구현 담당에게 돌아가며, 검토 담당이 APPROVED라고 쓰면 멈춥니다. 일반적인 요청은 에이전트 대신 자동으로 승인되지만, 위험한 요청(삭제, 푸시, 배포, 자격 증명, 프로젝트 밖의 모든 작업)은 여전히 사용자의 승인을 기다립니다.
- **넘기기.** 어떤 계획이나 답변이든 클릭 한 번으로 다른 에이전트에게 보냅니다.
- **릴레이.** 두 에이전트가 한쪽이 DONE이라고 말할 때까지 서로 대화합니다.

![계획, 구현, 검토 워크플로: Codex가 구현하고 Claude Code가 검토해 승인한 모습](../screenshots/readme/workflow.png)

### 계속 일하는 봇

봇은 계속 머무는 에이전트입니다. 이름, 역할, 지침, 에이전트와 모델(Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity 또는 내 모델)을 정해 주고 작업을 맡기세요. 다른 일을 하는 동안 봇이 백그라운드에서 작업을 처리합니다.

- **자기만의 자리.** 모든 봇은 자신의 폴더(`~/AgentDeck/Bots/<name>`)를 가지며, 작업에 프로젝트가 지정되지 않으면 그곳에서 일합니다. 어떤 작업이든 어떤 프로젝트로든 보낼 수 있고, 봇은 장소마다 대화를 따로 유지하므로 어디서 무엇을 했는지 기억합니다.
- **작업 대기열.** 봇이 바쁠 때도 작업을 추가할 수 있고, 봇은 한 번에 하나씩 실행합니다. 일시 정지, 다시 시작, 취소, 재시도가 가능하며, 끝난 작업마다 봇의 보고가 결과로 남습니다.
- **메모리.** 모든 봇은 자신의 폴더에 짧은 메모리 파일을 하나씩 둡니다. 작업을 시작할 때마다 이 파일을 읽고, 기억해 둘 만한 내용(컨벤션, 결정 사항, 무엇이 어디에 있는지, 선호하는 방식)을 적어 둡니다. 봇 페이지에서 읽고 편집할 수 있습니다.
- **일정.** "평일 9:00: 어제 병합된 풀 리퀘스트 검토", "6시간마다: 불안정한 테스트 찾기". 실행될 때마다 다른 작업처럼 대기열에 들어갑니다.
- **매니저 봇.** 봇은 AgentDeck 자체 도구(`list_bots`, `delegate_task`, `wait_for_tasks`)를 통해 작업의 일부를 다른 봇에게 넘기고 그 결과를 기다릴 수 있습니다. 리드 봇이 리뷰어, 테스터, 작성자에게 일을 나눠 주고 각자의 보고를 하나로 모을 수 있습니다.
- **무인 실행, 하지만 무방비는 아닙니다.** 봇이 작업을 진행하는 동안 봇 폴더와 해당 작업의 프로젝트 안에서 이루어지는 일반적인 요청은 봇 대신 자동으로 승인됩니다. 위험한 요청(삭제, 푸시, 배포, 자격 증명, 그 밖의 위치에서 하는 모든 작업)과 모든 질문은 여전히 사용자를 기다리며, 이때 봇은 사이드바에 "확인 필요"로 표시됩니다.
- **Telegram.** 내 Telegram 봇을 연결하고(@BotFather에서 받은 토큰은 키체인에 보관됩니다) 코드로 채팅을 페어링하세요. 그러면 휴대폰에서 작업을 보내고 봇의 답변을 그곳에서 받을 수 있습니다. 메시지를 봇 이름으로 시작하면("Reviewer: …") 특정 봇을 고를 수 있습니다. 봇에 확인이 필요하면 위험한 단계, 질문 또는 계획이 허용, 거부, 답변 선택 버튼과 함께 도착합니다. AgentDeck이 직접 Telegram에 메시지를 요청하므로 Mac의 어떤 것도 인터넷에 열리지 않으며, 페어링한 채팅에만 응답합니다.

다음 계획: 봇이 코드 이상의 일을 할 수 있도록 하는 연결 기능(메일, 캘린더, 브라우저, GitHub 등)과 봇을 시작하는 더 많은 방법(웹 페이지 변경, 메일 수신, GitHub 라벨).

![대기열의 작업을 처리 중인 봇, 일정과 메모리는 바로 옆 탭에](../screenshots/readme/bots.png)

### git 워크트리로 작업 격리

세션을 전용 워크트리와 브랜치에서 시작하면 에이전트가 내 작업 사본을 건드릴 수 없습니다. Git 보기에서 브랜치 그래프를 확인한 뒤, 워크트리를 다시 병합하거나(양쪽에서 같은 줄을 바꿨다면 명확하게 경고합니다) 버리면 됩니다.

![에이전트의 워크트리를 master에 다시 병합하는 화면](../screenshots/readme/worktree.png)

### 모든 명령을 손끝에서

메시지 상자에 `/`를 입력하면 해당 폴더에서 쓸 수 있는 에이전트 자체 명령(Claude Code 기본 명령, 사용자 지정 명령과 스킬, Gemini CLI 명령, Codex 프롬프트)과 AgentDeck 명령(`/plan`, `/terminal`, `/workflow`, `/git` …)이 나타납니다.

![슬래시 명령 메뉴](../screenshots/readme/slash.png)

### 그 밖의 기능

- **에이전트 설정.** `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, 스킬, 명령, 하위 에이전트를 전역 및 프로젝트별로 하나의 트리에서 편집합니다.
- **스마트 터미널.** 프로젝트 폴더에서 열리는 내 셸입니다. 선택한 부분이나 마지막 80줄에 대해 어떤 에이전트에게든 물어볼 수 있고, 클릭 한 번으로 "이 오류 설명"을 요청할 수도 있습니다.
- **무엇이든 첨부.** 파일이나 폴더를 끌어다 놓거나, 스크린샷을 붙여넣거나, 파일을 선택하세요. 이미지는 이미지 그대로 에이전트에게 전달됩니다.
- **내 모델.** DeepSeek, Kimi, GLM (Z.ai), Ollama, LM Studio, OpenRouter 또는 OpenAI·Anthropic 호환 엔드포인트를 같은 승인, 계획 모드, 워크플로와 함께 사용할 수 있습니다.
- **요금제 한도를 한눈에.** Claude Code와 Codex 세션에서는 메시지 상자 아래에 5시간 한도와 주간 한도를 얼마나 썼는지, 언제 초기화되는지가 표시됩니다.
- **막히는 곳에서 바로 로그인.** 에이전트에 로그인이 필요하면, 버튼 하나로 내장 터미널에서 해당 에이전트의 로그인 절차를 실행하고 메시지를 다시 보냅니다.
- **자동 업데이트.** 새 버전은 서명 및 공증을 거치며, 모든 업데이트는 설치 전에 앱에 내장된 키로 검증됩니다.

### 내 언어로

전체 인터페이스를 **English, Türkçe, Deutsch, Русский, Español, Français, Português, 简体中文, 日本語, 한국어, العربية**로 사용할 수 있으며, 아랍어는 오른쪽에서 왼쪽으로 읽는 레이아웃을 지원합니다. AgentDeck은 Mac의 언어를 따르며, 설정 → 화면 모드에서 직접 고를 수도 있습니다. 에이전트의 답변, 코드, 명령, 파일 이름은 절대 번역되지 않으며, 에이전트는 사용자가 쓴 언어로 답합니다.

![터키어와 아랍어(오른쪽에서 왼쪽)로 표시된 AgentDeck](../screenshots/readme/languages.png)

## Mac에 가볍게

AgentDeck은 네이티브 Tauri 앱입니다. 작은 Rust 코어와 macOS 자체 WebKit을 사용하며, 브라우저 엔진을 번들로 포함하지 않습니다.

| | Mac에서 측정(릴리스 빌드, 활성 상태 보기) |
|---|---|
| 다운로드 | 22 MB 디스크 이미지(Apple Silicon + Intel 통합 앱 하나) |
| 설치 크기 | 50 MB |
| 메모리, AgentDeck 자체 | 앱 ~31 MB + 인터페이스 ~207 MB + 시스템 헬퍼 ~18 MB ≈ **256 MB** |
| 유휴 시 CPU | **0.1 %**, 유휴 중 깨어남 없음 |
| 시작 | 인터페이스는 446 KB 메인 번들을 불러오며, 검토, 터미널, 갤러리, 설정은 열 때 불러옵니다 |

에이전트는 각자의 프로세스로 실행되며 각자의 메모리를 사용합니다. 작업 중에는 에이전트당 대략 140–210 MB입니다(Linux에서 측정: Claude Code ~188 MB, Codex ~159 MB). 이를 억제하기 위해 10분 동안 유휴 상태인 에이전트는 종료되고(다시 메시지를 쓰면 대화가 이어집니다), 동시에 최대 6개까지만 열려 있으며, 긴 기록은 80턴씩 나눠 불러옵니다.

## 지원 에이전트

| 에이전트 | AgentDeck의 통신 방식 | 로그인 |
|---|---|---|
| **Claude Code** | stream-json과 권한 프롬프트 도구를 사용하는 `claude` | Claude Pro, Max, Team 또는 Enterprise, 또는 Anthropic Console |
| **Codex** | `codex app-server`, stdio 기반 JSON-RPC | ChatGPT 요금제 또는 OpenAI API 키 |
| **Gemini CLI** | Agent Client Protocol | Google Workspace 또는 Cloud 계정, 또는 Gemini API 키 |
| **Grok Build** | Agent Client Protocol | xAI 계정 또는 API 키 |
| **Kimi Code** | `kimi acp`, Agent Client Protocol | Kimi Code 요금제 또는 Moonshot AI API 키 |
| **Antigravity** *(실험적 기능, 기본값은 꺼짐)* | `agy` 헤드리스 stream-json | Google 계정. Google 약관에 관한 설정의 안내를 참고하세요 |
| **내 모델** | AgentDeck 내장 에이전트, 또는 Anthropic 호환 API라면 Claude Code 내부에서 실행 | 키체인에 저장된 API 키 |

적어도 하나는 설치하세요. 첫 실행 화면에서 Mac에 이미 설치된 에이전트를 보여 주고, 나머지는 각 에이전트의 공식 명령으로 터미널에서 설치하거나 로그인합니다. 어떤 작업이 실행되는지 직접 볼 수 있습니다.

## 설치

1. [릴리스](https://github.com/kalismeras61/agentdeck/releases/latest)에서 최신 `AgentDeck-<version>.dmg`를 다운로드합니다.
2. AgentDeck을 응용 프로그램 폴더로 끌어다 놓고 엽니다. 서명 및 공증된 앱이라 더블 클릭으로 바로 열립니다.
3. **시작하기**를 따라 에이전트를 설정한 다음 프로젝트 폴더를 엽니다.

macOS 12 이상, Apple Silicon 또는 Intel.

## 작동 방식

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

- 각 에이전트는 세션 폴더에서 자식 프로세스로 실행되며, 로그인 셸의 환경을 그대로 사용합니다.
- 출력은 원본 그대로 저장된 뒤 하나의 이벤트 스트림(메시지, 도구 호출, diff, 승인, 질문, 계획)으로 정규화되고, 화면에 표시되기 전에 검증됩니다. 형식이 잘못된 내용은 명확한 오류로 표시될 뿐 절대 실행되지 않습니다.
- 승인 요청에는 사용자만 응답하며, 일반적인 요청에 한해 워크플로가 응답합니다. 위험한 작업은 절대 자동 승인되지 않습니다.
- 프런트엔드에는 셸 접근 권한이 없습니다. AgentDeck 자체의 범위가 좁은 명령만 호출할 수 있습니다.

## 소스에서 빌드

요구 사항: Node 22+, Rust 1.77+, Xcode 명령줄 도구.

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

릴리스(서명, 공증, 유니버설 빌드, 자동 업데이트)는 [docs/RELEASING.md](../RELEASING.md)에, 무엇이 구현되었고 각 부분을 어떻게 검증하는지는 [docs/STATUS.md](../STATUS.md)에, 에이전트 프로토콜은 [docs/PROVIDERS.md](../PROVIDERS.md)에 정리되어 있습니다.

<details>
<summary><b>키보드 단축키</b></summary>

| 동작 | 키 |
|---|---|
| 새 세션 | ⌘N |
| 세션 검색 | ⌘K |
| 사이드바 / 컨텍스트 패널 켜기/끄기 | ⌘B / ⌘. |
| 컨텍스트 탭 | ⌘1 … ⌘4 |
| 이전 / 다음 세션 | ⌥↑ / ⌥↓ |
| 메시지 상자로 이동 | ⌘L |
| 명령 | 메시지 맨 앞에 / |
| 기본 / 터미널 보기 | ⌘⇧R / ⌘⇧T |
| 모든 세션 / 검토 / 권한 / Git / 에이전트 설정 / 갤러리 | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| 설정 | ⌘, |
| 세션 이름 변경 | 제목을 더블 클릭하거나 사이드바에서 F2 |
| 검토: 다음/이전 파일, 변경 사항, 확인함으로 표시 | J / K, N / P, V |
| 보내기 / 줄 바꿈 | ↵ / ⇧↵ |

권한 요청에는 의도적으로 단축키를 두지 않았습니다.

</details>

<details>
<summary><b>프로젝트 구조</b></summary>

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

기술 스택: Tauri 2 (Rust), React 19, TypeScript (strict), Vite, Tailwind CSS 4, Radix UI, Zustand, Zod, Shiki, Monaco, xterm.js, SQLite.

</details>

## 기여하기

이슈와 풀 리퀘스트를 환영합니다. PR을 열기 전에 위의 검사를 실행하고, UI 문구는 짧고 쉽게 써 주세요.

**번역.** 인터페이스 번역은 AI의 도움을 받아 만들고 테스트로 검증했지만, 아직 원어민의 검토는 거치지 않았습니다. 그래서 수정 제안을 무척 환영합니다. `src/locales/<language>.json`을 편집한 뒤 `node scripts/i18n-validate.mjs`를 실행하세요. 새로운 인터페이스 문구는 `t()`를 거쳐야 합니다. 자세한 내용은 [docs/I18N.md](../I18N.md)를 참고하세요.

AgentDeck은 AGPL로, 그리고 이와 별도로 상용 라이선스로 제공됩니다. 이 방식을 유지하기 위해 기여자에게는 첫 풀 리퀘스트 때 기여자 라이선스 계약(CLA)에 서명해 달라고 요청합니다.

## 라이선스

AgentDeck은 [GNU Affero General Public License v3.0](../../LICENSE)에 따른 자유 소프트웨어입니다. 누구나 사용하고, 연구하고, 수정하고, 공유할 수 있습니다. 수정한 버전을 네트워크를 통해 다른 사람들이 사용하도록 운영하는 경우, 그 사람들에게 같은 라이선스로 해당 소스 코드를 제공해야 합니다.

AGPL의 의무가 없는 상용 라이선스가 필요하다면(예: AgentDeck을 비공개 제품에 포함해 배포하려는 경우) 저장소를 통해 연락해 주세요.

Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity는 각 소유자의 제품입니다. AgentDeck은 이들과 제휴 관계가 없고 이들의 보증을 받지도 않았으며, 사용자가 직접 설치한 사본을 실행할 뿐입니다.
