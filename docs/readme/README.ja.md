<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [Français](README.fr.md) · [Português](README.pt.md) · [简体中文](README.zh.md) · **日本語** · [한국어](README.ko.md) · [العربية](README.ar.md)

_これは翻訳版です。正式な内容は[英語の README](../../README.md) を参照してください。_

# AgentDeck

**いつも使っている AI コーディングエージェントを、ひとつの Mac ワークスペースに。**

Claude Code、Codex、Gemini CLI、Kimi Code、そして自分のモデルを並べて動かせます。エージェントの作業は整理された見やすい形で読め、すべての diff をレビューでき、質問や承認にはひとつの場所で答えられます。さらに、エージェント同士でプラン・実装・レビューを分担させることもできます。

使うのはあなたの CLI と、あなたのアカウント。API キーは不要です。

[Mac 版をダウンロード](https://github.com/kalismeras61/agentdeck/releases/latest) · [更新内容](https://github.com/kalismeras61/agentdeck/releases) · [仕組み](#仕組み) · [ソースからビルド](#ソースからビルド)

![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Built with Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck：コマンドの実行許可を求める Claude Code セッションと、右側にその変更の diff](../screenshots/readme/workspace.png)

</div>

## AgentDeck を使う理由

コーディングエージェントはターミナルでも十分便利です。ただし、5 つのプロジェクトで 3 つのエージェントを動かし始めるまでは。承認はスクロールで流れていき、diff はログに埋もれ、あるエージェントの作業を別のエージェントに確認させるにはコピー＆ペーストするしかありません。

AgentDeck はエージェントそのものには手を加えず、ちゃんとしたワークスペースを用意します。

- **本物の CLI を動かします。** `claude`、`codex`、`gemini` は自分でインストールしたものがそのまま起動し、PATH、設定、サインイン状態もターミナルと同じです。ターミナルで動くなら、ここでも動きます。
- **認証情報には一切触れません。** 各エージェントはそれぞれ独自の方法でサインインします。AgentDeck がトークンを読み取ったり、保存したり、送信したりすることはありません。自分のモデル用に追加した API キーは macOS のキーチェーンに保存されます。
- **すべてがあなたの Mac の中で完結します。** セッションはローカルの SQLite データベースに保存されます。アカウントも、テレメトリも、サーバーもありません。

## できること

### どのプロジェクトでも、どのエージェントとでも

プロジェクトはフォルダ、セッションはそこで作業する 1 つのエージェントです。最初のメッセージを送る前ならエージェントを切り替えられ、各エージェントが提供するすべてのモデルから選べ、会話の途中でもモデルを変更できます。プロジェクトなしでチャットを始めたり、セッションをプロジェクト間で移動したり、名前を変更したりできるほか、Claude Code、Codex、Gemini CLI が Mac にすでに保存しているセッションをインポートすることもできます。インポートしたセッションは同じ会話の続きから再開します。

エージェントがどれだけ深く考えるかも選べます。Claude Code と Codex では、モデルの横に**思考レベル**の設定があります（低から最大まで。そのモデルが対応しているレベルのみ表示）。メニューにはエージェントが現在使っている思考レベルが表示され、Claude Code の作業中でも変更はすぐに反映されます。新しいセッションの初期値は設定で指定できます。

### プルリクエストのように、変更をレビュー

エージェントが触れたファイルはすべて、ファイルごとに side-by-side または unified 形式の diff で表示されます。キーボードで移動でき、「確認済み」マークも付けられます。コミットやプッシュが勝手に行われることはありません。

エージェントが撮影したり参照したりしたスクリーンショットや画像（シミュレーターのスクリーンショット、レンダリングされたページ、読み込んだ画像など）は、会話の中と「プレビュー」タブにそのまま表示されます。クリックすると原寸大で確認できます。

![エージェントの変更を side-by-side でレビュー](../screenshots/readme/review.png)

### 承認、拒否、あるいは質問させる

あなたの許可が必要なコマンドや編集は、何を・どこで実行し、何に影響し、どの程度リスクがあるのかを示すカードとして表示されます。プランモードでは、エージェントは何かを変更する前にまずコードを読んで提案します。エージェントから質問があるとき（Claude Code の AskUserQuestion、Codex の request_user_input）は、ちゃんとしたフォームが表示されます。選択肢を 1 つ選ぶ、複数選ぶ、または自分で入力することができます。

![2 つの質問（単一選択と複数選択）をするエージェント](../screenshots/readme/question.png)

### エージェント同士を連携させる

- **ワークフロー：プラン → 実装 → レビュー。** 各ステージに好きなエージェントを割り当てられます。たとえば Claude Code がプランを立て、Codex が実装し、Claude Code がレビューする、といった具合です。各ステージは終わり次第すぐに次へ引き継ぎ、レビューの指摘はそのまま実装担当に戻され、レビュー担当が APPROVED と書いた時点で終了します。通常のリクエストはエージェントに代わって承認されますが、リスクのあるもの（削除、プッシュ、デプロイ、認証情報、プロジェクト外への操作など）は引き続きあなたの判断を待ちます。
- **引き継ぐ。** どんなプランや返信も、ワンクリックで別のエージェントに送れます。
- **リレー。** 2 つのエージェントが、どちらかが DONE と言うまで対話を続けます。

![プラン、実装、レビューのワークフロー：Codex が実装し、Claude Code がレビューして承認](../screenshots/readme/workflow.png)

### 働き続けるボット

ボットは、常駐するエージェントです。名前、役割、指示、エージェントとモデル（Claude Code、Codex、Gemini CLI、Grok Build、Kimi Code、Antigravity、または自分のモデル）を決めたら、あとはタスクを渡すだけ。あなたが別の作業をしているあいだに、バックグラウンドで順に片付けていきます。

- **自分の居場所。** 各ボットは専用のフォルダ（`~/AgentDeck/Bots/<name>`）を持ち、タスクでプロジェクトが指定されない限りそこで作業します。どのタスクもどのプロジェクトに向けることもでき、ボットは場所ごとに別々の会話を保つので、どこで何をしたかを覚えています。
- **タスクキュー。** ボットが作業中でもタスクを追加でき、1 つずつ実行されます。一時停止、再開、キャンセル、再試行ができ、完了したタスクにはそれぞれボットの報告が結果として残ります。
- **メモリ。** 各ボットは自分のフォルダに短いメモリファイルを持っています。タスクを始めるたびにこれを読み、覚えておく価値のあること（規約、決定事項、どこに何があるか、あなたの好み）を書き留めます。内容はボットのページで確認・編集できます。
- **スケジュール。** 「平日 9:00：昨日マージされたプルリクエストをレビュー」「6 時間ごと：不安定なテストを探す」。実行のたびに通常のタスクと同じくキューに入ります。
- **マネージャーボット。** ボットは AgentDeck 独自のツール（`list_bots`、`delegate_task`、`wait_for_tasks`）を使って、タスクの一部を他のボットに任せ、その結果を待つことができます。リーダー役のボットが、レビュー担当、テスト担当、執筆担当に作業を割り振り、それぞれの報告をまとめることもできます。
- **無人でも、ノーチェックではない。** ボットがタスクに取り組んでいる間、そのフォルダとタスクのプロジェクト内での通常のリクエストはボットに代わって承認されます。リスクのある操作（削除、プッシュ、デプロイ、認証情報、それ以外の場所での操作など）とすべての質問はあなたの判断を待ち、ボットはサイドバーに「対応が必要」と表示されます。

今後の予定：スマートフォンや Telegram からのボットへのアクセス。

![キューのタスクを順に処理するボット。スケジュールとメモリはすぐ隣のタブに](../screenshots/readme/bots.png)

### git ワークツリーで作業を分離

専用のワークツリーとブランチでセッションを開始すれば、エージェントがあなたの作業コピーに干渉することはありません。Git ビューでブランチのグラフを確認し、ワークツリーをマージして戻す（双方が同じ行を変更している場合ははっきりと警告されます）か、そのまま破棄できます。

![エージェントのワークツリーを master にマージ](../screenshots/readme/worktree.png)

### あらゆるコマンドをすぐに

メッセージ欄に `/` と入力すると、そのフォルダでエージェントが使えるコマンド（Claude Code の組み込みコマンド、自作のカスタムコマンドとスキル、Gemini CLI のコマンド、Codex のプロンプト）と、AgentDeck 独自のコマンド（`/plan`、`/terminal`、`/workflow`、`/git` …）が表示されます。

![スラッシュコマンドのメニュー](../screenshots/readme/slash.png)

### その他の機能

- **エージェント設定。** `CLAUDE.md`、`AGENTS.md`、`GEMINI.md`、スキル、コマンド、サブエージェントを、グローバル設定もプロジェクトごとの設定も、ひとつのツリーで編集できます。
- **スマートターミナル。** プロジェクトフォルダで自分のシェルが使えます。選択範囲や直近 80 行についてどのエージェントにも質問でき、ワンクリックの「このエラーを説明して」も用意しています。
- **何でも添付。** ファイルやフォルダをドロップしたり、スクリーンショットを貼り付けたり、ファイルを選んだりできます。画像は画像のままエージェントに届きます。
- **あなたのモデル。** DeepSeek、Kimi、GLM (Z.ai)、Ollama、LM Studio、OpenRouter、または OpenAI 互換・Anthropic 互換の任意のエンドポイントを、同じ承認、プランモード、ワークフローで使えます。
- **プランの使用上限をひと目で。** Claude Code と Codex のセッションでは、メッセージ欄の下に 5 時間と週間の上限をどれだけ使ったか、いつリセットされるかが表示されます。
- **サインインが切れたらその場で。** エージェントにログインが必要になると、ボタンひとつで内蔵ターミナル上でそのエージェント自身のサインインを実行し、メッセージを再送信します。
- **自動アップデート。** 新しいバージョンは署名と公証が済んでおり、各アップデートはインストール前にアプリに組み込まれた鍵で検証されます。

### あなたの言語で

インターフェース全体が **English、Türkçe、Deutsch、Русский、Español、Français、Português、简体中文、日本語、한국어、العربية** に対応しており、アラビア語では右から左へのレイアウトになります。AgentDeck は Mac の言語設定に従いますが、設定 → 外観 で選ぶこともできます。エージェントの返信、コード、コマンド、ファイル名が翻訳されることはなく、エージェントはあなたが書いた言語で返答します。

![トルコ語とアラビア語（右から左）の AgentDeck](../screenshots/readme/languages.png)

## Mac に軽い

AgentDeck はネイティブの Tauri アプリです。小さな Rust コアと macOS 標準の WebKit で動き、ブラウザエンジンを同梱していません。

| | Mac での実測値（リリースビルド、アクティビティモニタ） |
|---|---|
| ダウンロード | 22 MB のディスクイメージ（Apple Silicon と Intel を 1 つのアプリに統合） |
| インストール後 | 50 MB |
| メモリ（AgentDeck 本体） | アプリ 約 31 MB + インターフェース 約 207 MB + システムヘルパー 約 18 MB ≈ **256 MB** |
| アイドル時の CPU | **0.1 %**、アイドル時のウェイクアップなし |
| 起動 | インターフェースは 446 KB のメインバンドルを読み込み、レビュー、ターミナル、ギャラリー、設定は開いたときに読み込まれます |

エージェントはそれぞれ独立したプロセスとして動き、自分のメモリを使います。作業中は 1 つあたりおよそ 140–210 MB です（Linux での実測：Claude Code 約 188 MB、Codex 約 159 MB）。これを抑えるため、10 分間アイドル状態のエージェントは終了され（再びメッセージを書けば会話は続きます）、同時に開いておけるのは最大 6 つまでで、長い履歴は 80 ターンずつ読み込まれます。

## 対応エージェント

| エージェント | AgentDeck との通信方法 | サインイン |
|---|---|---|
| **Claude Code** | `claude`、stream-json と権限プロンプトツール | Claude Pro、Max、Team、Enterprise、または Anthropic Console |
| **Codex** | `codex app-server`、stdio 経由の JSON-RPC | ChatGPT のプランまたは OpenAI API キー |
| **Gemini CLI** | Agent Client Protocol | Google Workspace または Cloud アカウント、もしくは Gemini API キー |
| **Grok Build** | Agent Client Protocol | xAI アカウントまたは API キー |
| **Kimi Code** | `kimi acp`、Agent Client Protocol | Kimi Code のプランまたは Moonshot AI API キー |
| **Antigravity** *（実験的、デフォルトでオフ）* | `agy` ヘッドレス stream-json | Google アカウント。Google の規約に関する設定内の注意書きを参照してください |
| **あなたのモデル** | AgentDeck の組み込みエージェント、または Anthropic 互換 API の場合は Claude Code 内で実行 | キーチェーン内の API キー |

少なくとも 1 つはインストールしてください。初回起動画面には Mac にインストール済みのエージェントが表示され、残りはターミナルで各エージェントの公式コマンドを使ってインストールまたはサインインします。何が実行されているかはその場で確認できます。

## インストール

1. [Releases](https://github.com/kalismeras61/agentdeck/releases/latest) から最新の `AgentDeck-<version>.dmg` をダウンロードします。
2. AgentDeck を「アプリケーション」にドラッグして開きます。署名と公証が済んでいるので、ダブルクリックで開けます。
3. **はじめる** の手順に沿ってエージェントを設定し、プロジェクトフォルダを開きます。

macOS 12 以降、Apple Silicon または Intel。

## 仕組み

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

- 各エージェントは、セッションのフォルダで子プロセスとして、ログインシェルの環境のまま動きます。
- 出力はそのまま保存されたうえで 1 本のイベントストリーム（メッセージ、ツール呼び出し、diff、承認、質問、プラン）に正規化され、何かを描画する前に検証されます。不正な形式のものは明確なエラーとして表示され、実行されることはありません。
- 承認に答えるのはあなただけです。通常のリクエストに限りワークフローが答えることもありますが、危険な操作が自動承認されることはありません。
- フロントエンドはシェルにアクセスできません。呼び出せるのは、範囲を絞った AgentDeck 独自のコマンドだけです。

## ソースからビルド

必要なもの：Node 22 以上、Rust 1.77 以上、Xcode コマンドラインツール。

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

リリース（署名、公証、ユニバーサル、自動アップデート）については [docs/RELEASING.md](../RELEASING.md)、何が実装済みでそれぞれどう検証しているかについては [docs/STATUS.md](../STATUS.md)、エージェントのプロトコルについては [docs/PROVIDERS.md](../PROVIDERS.md) を参照してください。

<details>
<summary><b>キーボードショートカット</b></summary>

| 操作 | キー |
|---|---|
| 新規セッション | ⌘N |
| セッションを検索 | ⌘K |
| サイドバー / コンテキストパネルの表示を切り替え | ⌘B / ⌘. |
| コンテキストのタブ | ⌘1 … ⌘4 |
| 前／次のセッション | ⌥↑ / ⌥↓ |
| メッセージ欄にフォーカス | ⌘L |
| コマンド | メッセージの先頭で / |
| リッチ表示／ターミナル表示 | ⌘⇧R / ⌘⇧T |
| すべてのセッション / レビュー / 権限 / Git / エージェント設定 / ギャラリー | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| 設定 | ⌘, |
| セッションの名前を変更 | タイトルをダブルクリック、またはサイドバーで F2 |
| レビュー：次/前のファイル、変更、確認済みにする | J / K、N / P、V |
| 送信 / 改行 | ↵ / ⇧↵ |

許可リクエストには、意図的にショートカットを用意していません。

</details>

<details>
<summary><b>プロジェクト構成</b></summary>

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

技術スタック：Tauri 2 (Rust)、React 19、TypeScript (strict)、Vite、Tailwind CSS 4、Radix UI、Zustand、Zod、Shiki、Monaco、xterm.js、SQLite。

</details>

## コントリビュート

Issue やプルリクエストを歓迎します。PR を出す前に上記のチェックを実行し、UI のテキストは短く平易に保ってください。

**翻訳について。** インターフェースの翻訳は AI の助けを借りて作成し、テストでチェックしていますが、ネイティブスピーカーによる確認はまだです。修正は大歓迎です。`src/locales/<language>.json` を編集し、`node scripts/i18n-validate.mjs` を実行してください。新しいインターフェースのテキストは `t()` を通します。詳しくは [docs/I18N.md](../I18N.md) を参照してください。

AgentDeck は AGPL のもとで提供されると同時に、別途商用ライセンスでも提供されています。これを維持するため、コントリビューターには最初のプルリクエストの際にコントリビューターライセンス契約（CLA）への署名をお願いしています。

## ライセンス

AgentDeck は [GNU Affero General Public License v3.0](../../LICENSE) のもとで提供されるフリーソフトウェアです。使用、調査、改変、共有が自由に行えます。改変したバージョンをネットワーク経由で他者に提供する場合は、そのソースコードを同じライセンスのもとで提供しなければなりません。

AGPL の義務を伴わない商用ライセンス（たとえばクローズドな製品に AgentDeck を組み込んで出荷する場合）が必要な場合は、リポジトリを通じてご連絡ください。

Claude Code、Codex、Gemini CLI、Grok Build、Kimi Code、Antigravity は、それぞれの所有者の製品です。AgentDeck はこれらと提携しておらず、また推奨を受けているものでもありません。AgentDeck はあなた自身がインストールしたものを実行します。
