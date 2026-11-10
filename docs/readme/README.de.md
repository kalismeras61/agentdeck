<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · **Deutsch** · [Русский](README.ru.md) · [Español](README.es.md) · [Français](README.fr.md) · [Português](README.pt.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [العربية](README.ar.md)

_Dies ist eine Übersetzung; maßgeblich ist die [englische README](../../README.md)._

# AgentDeck

**Ein Mac-Arbeitsbereich für alle KI-Coding-Agenten, die du schon nutzt.**

Lass Claude Code, Codex, Gemini CLI, Kimi Code und deine eigenen Modelle nebeneinander laufen. Lies ihre Arbeit als saubere, strukturierte Ausgabe, prüfe jeden Diff, beantworte ihre Fragen und Freigaben an einem Ort – und lass sie planen, umsetzen und die Arbeit der anderen prüfen.

Deine CLIs. Deine Konten. Keine API-Schlüssel nötig.

[Für Mac herunterladen](https://github.com/kalismeras61/agentdeck/releases/latest) · [Neuigkeiten](https://github.com/kalismeras61/agentdeck/releases) · [Wie es funktioniert](#wie-es-funktioniert) · [Aus dem Quellcode bauen](#aus-dem-quellcode-bauen)

![Lizenz: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Gebaut mit Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck: Eine Claude-Code-Sitzung möchte einen Befehl ausführen, rechts der Diff ihrer Änderungen](../screenshots/readme/workspace.png)

</div>

## Warum AgentDeck

Coding-Agenten sind im Terminal großartig – bis du drei davon in fünf Projekten gleichzeitig laufen lässt. Freigabeanfragen scrollen weg, Diffs gehen in Logs unter, und damit ein Agent die Arbeit eines anderen prüft, heißt es: kopieren und einfügen.

AgentDeck lässt die Agenten genau so, wie sie sind, und gibt ihnen einen richtigen Arbeitsbereich:

- **Es startet die echten CLIs.** `claude`, `codex` und `gemini` laufen aus deiner eigenen Installation, mit demselben PATH, derselben Konfiguration und derselben Anmeldung wie im Terminal. Was in deinem Terminal funktioniert, funktioniert auch hier.
- **Es rührt deine Zugangsdaten nie an.** Jeder Agent meldet sich über seinen eigenen Ablauf an. AgentDeck liest, speichert und sendet keine Tokens; die API-Schlüssel, die du für deine eigenen Modelle hinzufügst, bleiben im macOS-Schlüsselbund.
- **Alles bleibt auf deinem Mac.** Sitzungen liegen in einer lokalen SQLite-Datenbank. Kein Konto, keine Telemetrie, kein Server.

## Was du damit machen kannst

### Mit jedem Agenten in jedem Projekt arbeiten

Jedes Projekt ist ein Ordner; jede Sitzung ist ein Agent, der darin arbeitet. Wechsle den Agenten vor der ersten Nachricht, wähle aus allen Modellen, die ein Agent anbietet, und wechsle das Modell mitten im Gespräch. Starte einen Chat ohne Projekt, verschiebe Sitzungen zwischen Projekten, benenne sie um oder importiere die Sitzungen, die Claude Code, Codex und Gemini CLI bereits auf deinem Mac gespeichert haben; sie setzen dasselbe Gespräch fort.

Du bestimmst auch, wie gründlich der Agent nachdenkt. Claude Code und Codex haben neben dem Modell eine Einstellung für den **Aufwand** (von niedrig bis maximal, nur die Stufen, die das jeweilige Modell unterstützt). Das Menü zeigt den Aufwand, den der Agent gerade nutzt; eine Änderung greift sofort, auch während Claude Code arbeitet, und in den Einstellungen legst du fest, mit welchem Aufwand neue Sitzungen starten.

### Änderungen prüfen wie einen Pull Request

Jede Datei, die ein Agent anfasst, erscheint als Diff – nebeneinander oder einheitlich, Datei für Datei, mit Tastaturnavigation und „Gesehen“-Markierungen. Nichts wird für dich committet oder gepusht.

Bildschirmfotos und Bilder, die ein Agent aufnimmt oder sich ansieht (ein Screenshot aus dem Simulator, eine gerenderte Seite, ein Bild, das er liest), erscheinen direkt im Gespräch und im Tab „Vorschau“; ein Klick zeigt sie in voller Größe.

![Die Änderungen eines Agenten nebeneinander prüfen](../screenshots/readme/review.png)

### Freigeben, ablehnen oder fragen lassen

Befehle und Bearbeitungen, die dein OK brauchen, erscheinen als Karten, die zeigen, was ausgeführt wird, wo, was es betrifft und wie riskant es ist. Im Planmodus kann ein Agent lesen und Vorschläge machen, bevor er irgendetwas ändert. Hat ein Agent eine Frage (AskUserQuestion bei Claude Code, request_user_input bei Codex), bekommst du ein echtes Formular: Wähle eine Option, mehrere oder schreib deine eigene Antwort.

![Ein Agent stellt zwei Fragen, eine mit Einfach- und eine mit Mehrfachauswahl](../screenshots/readme/question.png)

### Agenten zusammenarbeiten lassen

- **Workflows: Plan → Umsetzung → Prüfung.** Setze für jede Phase einen beliebigen Agenten ein, zum Beispiel: Claude Code plant, Codex setzt um, Claude Code prüft. Jede Phase übergibt, sobald sie fertig ist, Prüfhinweise gehen direkt zurück an den umsetzenden Agenten, und der Workflow endet, sobald der Prüfer APPROVED schreibt. Gewöhnliche Anfragen werden für die Agenten freigegeben; riskante (Löschen, Pushen, Deployen, Zugangsdaten, alles außerhalb des Projekts) warten weiterhin auf dich.
- **Übergeben.** Schick einen Plan oder eine Antwort mit einem Klick an einen anderen Agenten.
- **Relay.** Zwei Agenten sprechen miteinander, bis einer DONE sagt.

![Ein Workflow aus Plan, Umsetzung und Prüfung: Codex hat umgesetzt, Claude Code hat geprüft und freigegeben](../screenshots/readme/workflow.png)

### Bots, die weiterarbeiten

Ein Bot ist ein Agent, der bleibt. Gib ihm einen Namen, einen Job, Anweisungen und einen Agenten samt Modell (Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity oder deine eigenen Modelle), dann übergib ihm Aufgaben. Er arbeitet sie im Hintergrund ab, während du etwas anderes machst.

- **Ein eigener Ort.** Jeder Bot hat seinen eigenen Ordner (`~/AgentDeck/Bots/<name>`) und arbeitet dort, solange eine Aufgabe kein Projekt nennt. Du kannst jede Aufgabe auf jedes Projekt richten; der Bot führt für jeden Ort eine eigene Unterhaltung und weiß so, was er wo getan hat.
- **Eine Warteschlange für Aufgaben.** Füg Aufgaben hinzu, während der Bot beschäftigt ist; er erledigt sie nacheinander. Pausieren, fortsetzen, abbrechen oder erneut versuchen – und jede erledigte Aufgabe behält den Bericht des Bots als Ergebnis.
- **Gedächtnis.** Jeder Bot führt eine kurze Gedächtnisdatei in seinem Ordner. Er liest sie zu Beginn jeder Aufgabe und notiert, was sich zu merken lohnt: Konventionen, Entscheidungen, wo was liegt, was du bevorzugst. Du kannst sie auf der Seite des Bots lesen und bearbeiten.
- **Zeitpläne.** „Werktags um 9:00: die gestern gemergten Pull Requests reviewen“, „alle 6 Stunden: nach flaky Tests suchen“. Jeder Lauf reiht sich wie jede Aufgabe in die Warteschlange ein.
- **Ein Manager-Bot.** Bots können Teile einer Aufgabe über AgentDecks eigene Tools (`list_bots`, `delegate_task`, `wait_for_tasks`) an andere Bots abgeben und auf deren Ergebnisse warten. Ein leitender Bot kann die Arbeit auf einen Reviewer, einen Tester und einen Autor verteilen und ihre Berichte zusammenführen.
- **Orchestrierung über Modelle hinweg.** Ein Modell leitet, die anderen erledigen die Arbeit: Ein Manager auf deinem stärksten Modell plant und prüft, Builder auf Opus, Sonnet, Codex oder Gemini schreiben den Code. Gib einem Bot mehrere Modelle zur Auswahl, und sein Manager wählt pro Aufgabe (ein schnelleres Modell für Einfaches, ein stärkeres für Schweres, dazu den Aufwand, mit dem es nachdenkt). Schalte „Jede Aufgabe in einem eigenen Worktree“ ein, und jede Aufgabe arbeitet auf einem frischen Branch, sodass mehrere Builder gleichzeitig an einem Repository arbeiten können, ohne sich in die Quere zu kommen. Jeder Bot zeigt den Verbrauch seiner Aufgaben (Aufgaben, Tokens, Kosten zu API-Preisen) neben den Plan-Limits seines Agenten.
- **Unbeaufsichtigt, aber nicht unkontrolliert.** Während ein Bot an einer Aufgabe arbeitet, werden gewöhnliche Anfragen innerhalb seines Ordners und des Projekts der Aufgabe für ihn freigegeben. Alles Riskante (Löschen, Pushen, Deployen, Zugangsdaten, alles anderswo) und jede Frage wartet auf dich, und der Bot zeigt in der Seitenleiste „Braucht dich“.
- **Telegram.** Verbinde einen eigenen Telegram-Bot (ein Token von @BotFather, das in deinem Schlüsselbund liegt) und kopple deinen Chat mit einem Code. Dann schreibst du eine Aufgabe vom Handy aus, und die Antwort des Bots kommt dort an; beginne mit dem Namen eines Bots („Reviewer: …“), um ihn auszuwählen. Wenn ein Bot dich braucht, kommen der riskante Schritt, die Frage oder der Plan mit Buttons: erlauben, ablehnen, eine Antwort wählen. AgentDeck fragt Telegram selbst nach Nachrichten, deshalb ist auf deinem Mac nichts vom Internet aus erreichbar, und nur Chats, die du gekoppelt hast, bekommen eine Antwort.

- **Verbindungen.** Bots können mehr als Code. Füge unter Einstellungen → Verbindungen einen Browser (Playwright), GitHub, Notion, Kalender, Erinnerungen, Mail und Notizen auf deinem Mac (über die Apps und die Konten, die du dort schon hast) oder einen beliebigen MCP-Server per Befehl oder Adresse hinzu, jeweils mit deinem eigenen Konto; Tokens bleiben in deinem Schlüsselbund. Dann wählst du pro Bot, was er nutzen darf. Während ein Bot an einer Aufgabe arbeitet, ist erlaubt, was eine Verbindung liest; was sie ändert (ein Pull Request gemergt, ein Termin hinzugefügt, ein Mail-Entwurf geöffnet), wartet auf dich, es sei denn, du lässt diese Verbindung selbstständig handeln. Ein Bot verschickt nie Mails.
- **Trigger.** Neben Zeitplänen und Telegram bekommt ein Bot Arbeit, wenn sich eine Webseite ändert (ein Preis, eine Ankündigung), neue Dateien in einem Ordner landen, ein Issue oder Pull Request in deinem Repository das Label `agentdeck` bekommt oder ein Skript auf deinem Mac seinen Hook aufruft (`curl -X POST http://127.0.0.1:47810/hooks/…`). Bots können dir außerdem mit `notify_me` sofort etwas mitteilen, auf Telegram und als Mitteilung.
- **Fertige Bots.** Starte mit einem Rechercheur, einem Posteingangs-Assistenten, einem Tagesplaner, einem Social-Media-Texter, einem Wächter oder einem Entwickler und ändere, was du möchtest.

![Ein Bot arbeitet seine Warteschlange ab, seine Zeitpläne und sein Gedächtnis sind nur einen Tab entfernt](../screenshots/readme/bots.png)

### Arbeit in Git-Worktrees isolieren

Starte eine Sitzung in einem eigenen Worktree und Branch, damit ein Agent deiner Arbeitskopie nicht in die Quere kommt. Sieh dir den Branch-Graphen in der Git-Ansicht an und führe den Worktree dann zurück zusammen (mit einer klaren Warnung, wenn beide Seiten dieselben Zeilen geändert haben) oder verwirf ihn.

![Den Worktree eines Agenten zurück in master zusammenführen](../screenshots/readme/worktree.png)

### Jeder Befehl griffbereit

Tippe `/` ins Nachrichtenfeld, und du siehst die eigenen Befehle des Agenten in diesem Ordner (die eingebauten von Claude Code, deine eigenen Befehle und Skills, die Befehle von Gemini CLI, deine Codex-Prompts) sowie die von AgentDeck (`/plan`, `/terminal`, `/workflow`, `/git` …).

![Das Slash-Befehlsmenü](../screenshots/readme/slash.png)

### Und außerdem

- **Agent-Einrichtung.** Bearbeite `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, Skills, Befehle und Subagenten – global und pro Projekt – in einem einzigen Baum.
- **Smartes Terminal.** Deine eigene Shell im Projektordner. Frag einen beliebigen Agenten zur Auswahl oder zu den letzten 80 Zeilen, inklusive „Diesen Fehler erklären“ per Klick.
- **Alles anhängen.** Zieh Dateien oder Ordner hinein, füge ein Bildschirmfoto ein oder wähle Dateien aus. Bilder kommen beim Agenten als Bilder an.
- **Deine eigenen Modelle.** DeepSeek, Kimi, GLM (Z.ai), Ollama, LM Studio, OpenRouter oder jeder OpenAI- bzw. Anthropic-kompatible Endpunkt, mit denselben Freigaben, demselben Planmodus und denselben Workflows.
- **Plan-Limits auf einen Blick.** Unter dem Nachrichtenfeld zeigen Claude-Code- und Codex-Sitzungen, wie viel von deinem 5-Stunden- und Wochenlimit du verbraucht hast und wann sie zurückgesetzt werden.
- **Anmelden, wo es hakt.** Braucht ein Agent eine Anmeldung, startet ein Button dessen eigene Anmeldung in einem eingebauten Terminal und sendet deine Nachricht erneut.
- **Aktualisiert sich selbst.** Neue Versionen sind signiert und notarisiert, und jedes Update wird vor der Installation mit einem in die App eingebauten Schlüssel geprüft.

### In deiner Sprache

Die gesamte Oberfläche gibt es auf **English, Türkçe, Deutsch, Русский, Español, Français, Português, 简体中文, 日本語, 한국어 und العربية**, mit Rechts-nach-links-Layout für Arabisch. AgentDeck folgt der Sprache deines Macs, oder du wählst eine unter Einstellungen → Darstellung. Antworten der Agenten, Code, Befehle und Dateinamen werden nie übersetzt, und die Agenten antworten in der Sprache, in der du ihnen schreibst.

![AgentDeck auf Türkisch und auf Arabisch, von rechts nach links](../screenshots/readme/languages.png)

## Schont deinen Mac

AgentDeck ist eine native Tauri-App: ein kleiner Rust-Kern und das systemeigene WebKit von macOS, keine mitgelieferte Browser-Engine.

| | Gemessen auf einem Mac (Release-Build, Aktivitätsanzeige) |
|---|---|
| Download | 22 MB Disk-Image (Apple Silicon + Intel in einer App) |
| Installiert | 50 MB |
| Arbeitsspeicher, nur AgentDeck | ~31 MB App + ~207 MB Oberfläche + ~18 MB System-Hilfsprozesse ≈ **256 MB** |
| CPU im Leerlauf | **0,1 %**, keine Aufwachvorgänge im Leerlauf |
| Start | Die Oberfläche lädt ein 446 KB großes Haupt-Bundle; Prüfung, Terminal, Galerie und Einstellungen werden erst beim Öffnen geladen |

Die Agenten laufen als eigene Prozesse mit eigenem Arbeitsspeicher, während der Arbeit grob 140–210 MB pro Agent (gemessen unter Linux: Claude Code ~188 MB, Codex ~159 MB). Damit das im Rahmen bleibt, wird ein Agent nach 10 Minuten Leerlauf beendet (sein Gespräch geht weiter, sobald du wieder schreibst), höchstens 6 bleiben gleichzeitig offen, und lange Verläufe werden in Blöcken von 80 Gesprächsrunden geladen.

## Unterstützte Agenten

| Agent | Wie AgentDeck mit ihm spricht | Anmeldung |
|---|---|---|
| **Claude Code** | `claude` mit stream-json und seinem Permission-Prompt-Tool | Claude Pro, Max, Team oder Enterprise oder Anthropic Console |
| **Codex** | `codex app-server`, JSON-RPC über stdio | ChatGPT-Abo oder OpenAI-API-Schlüssel |
| **Gemini CLI** | Agent Client Protocol | Google-Workspace- oder Cloud-Konto oder ein Gemini-API-Schlüssel |
| **Grok Build** | Agent Client Protocol | xAI-Konto oder API-Schlüssel |
| **Kimi Code** | `kimi acp`, Agent Client Protocol | Kimi-Code-Abo oder Moonshot-AI-API-Schlüssel |
| **Antigravity** *(experimentell, standardmäßig aus)* | `agy` headless stream-json | Google-Konto; beachte den Hinweis in den Einstellungen zu Googles Nutzungsbedingungen |
| **Deine Modelle** | Der eingebaute Agent von AgentDeck oder, für Anthropic-kompatible APIs, innerhalb von Claude Code | API-Schlüssel in deinem Schlüsselbund |

Installiere mindestens einen. Der Startbildschirm beim ersten Öffnen zeigt, welche bereits auf deinem Mac sind, und installiert die übrigen bzw. meldet dich bei ihnen an – mit dem offiziellen Befehl des jeweiligen Agenten, im Terminal, wo du siehst, was er tut.

## Installation

1. Lade die neueste `AgentDeck-<version>.dmg` unter [Releases](https://github.com/kalismeras61/agentdeck/releases/latest) herunter.
2. Zieh AgentDeck in den Ordner „Programme“ und öffne es. Die App ist signiert und notarisiert und öffnet sich deshalb per Doppelklick.
3. Folge **Loslegen**, um einen Agenten einzurichten, und öffne dann einen Projektordner.

macOS 12 oder neuer, Apple Silicon oder Intel.

## Wie es funktioniert

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

- Jeder Agent läuft als Kindprozess im Ordner der Sitzung, mit der Umgebung deiner Login-Shell.
- Seine Ausgabe wird unverändert gespeichert, dann in einen einheitlichen Event-Stream normalisiert (Nachrichten, Tool-Aufrufe, Diffs, Freigaben, Fragen, Pläne) und validiert, bevor irgendetwas dargestellt wird. Alles Fehlerhafte wird als klarer Fehler angezeigt, nie ausgeführt.
- Freigaben erteilst nur du – oder bei gewöhnlichen Anfragen ein Workflow; nichts Gefährliches wird automatisch freigegeben.
- Das Frontend hat keinen Shell-Zugriff: Es kann nur die eigenen, eng gefassten Befehle von AgentDeck aufrufen.

## Aus dem Quellcode bauen

Voraussetzungen: Node 22+, Rust 1.77+ und die Xcode Command Line Tools.

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

Releases (signiert, notarisiert, universal, selbstaktualisierend) sind in [docs/RELEASING.md](../RELEASING.md) beschrieben; was fertig ist und wie jeder Teil geprüft wird, in [docs/STATUS.md](../STATUS.md); die Agent-Protokolle in [docs/PROVIDERS.md](../PROVIDERS.md).

<details>
<summary><b>Tastenkürzel</b></summary>

| Aktion | Tasten |
|---|---|
| Neue Sitzung | ⌘N |
| Sitzungen durchsuchen | ⌘K |
| Seitenleiste / Kontextbereich ein- und ausblenden | ⌘B / ⌘. |
| Kontext-Tabs | ⌘1 … ⌘4 |
| Vorherige / nächste Sitzung | ⌥↑ / ⌥↓ |
| Nachrichtenfeld fokussieren | ⌘L |
| Befehle | / am Anfang einer Nachricht |
| Formatierte / Terminal-Ansicht | ⌘⇧R / ⌘⇧T |
| Alle Sitzungen / Prüfung / Berechtigungen / Git / Agent-Einrichtung / Galerie | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| Einstellungen | ⌘, |
| Sitzung umbenennen | Doppelklick auf den Titel oder F2 in der Seitenleiste |
| Prüfung: nächste/vorherige Datei, Änderung, als gesehen markieren | J / K, N / P, V |
| Senden / neue Zeile | ↵ / ⇧↵ |

Berechtigungsanfragen haben bewusst kein Tastenkürzel.

</details>

<details>
<summary><b>Projektstruktur</b></summary>

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

Tech-Stack: Tauri 2 (Rust), React 19, TypeScript (strict), Vite, Tailwind CSS 4, Radix UI, Zustand, Zod, Shiki, Monaco, xterm.js, SQLite.

</details>

## Mitwirken

Issues und Pull Requests sind willkommen. Führe vor einem PR die Checks oben aus, und halte Texte in der Oberfläche kurz und schlicht.

**Übersetzungen.** Die Übersetzungen der Oberfläche wurden mithilfe von KI erstellt und durch Tests geprüft, aber noch nicht von Muttersprachlern – Korrekturen sind daher sehr willkommen: Bearbeite `src/locales/<language>.json` und führe `node scripts/i18n-validate.mjs` aus. Neue Oberflächentexte laufen über `t()`; siehe [docs/I18N.md](../I18N.md).

AgentDeck wird unter der AGPL und separat unter einer kommerziellen Lizenz angeboten. Damit das möglich bleibt, werden Mitwirkende gebeten, bei ihrem ersten Pull Request eine Contributor License Agreement (CLA) zu unterzeichnen.

## Lizenz

AgentDeck ist freie Software unter der [GNU Affero General Public License v3.0](../../LICENSE). Du darfst sie nutzen, untersuchen, verändern und weitergeben. Wenn du eine veränderte Version für andere über ein Netzwerk betreibst, musst du ihnen deren Quellcode unter derselben Lizenz anbieten.

Für eine kommerzielle Lizenz ohne die Pflichten der AGPL (zum Beispiel, um AgentDeck in einem geschlossenen Produkt auszuliefern) melde dich über das Repository.

Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code und Antigravity sind Produkte ihrer jeweiligen Inhaber. AgentDeck ist weder mit ihnen verbunden noch von ihnen empfohlen; es führt deine eigenen installierten Kopien aus.
