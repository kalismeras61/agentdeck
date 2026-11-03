<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · **Français** · [Português](README.pt.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [العربية](README.ar.md)

_Ceci est une traduction ; le texte de référence est le [README en anglais](../../README.md)._

# AgentDeck

**Un seul espace de travail sur Mac pour tous les agents de code IA que vous utilisez déjà.**

Faites tourner Claude Code, Codex, Gemini CLI, Kimi Code et vos propres modèles côte à côte. Lisez leur travail sous forme de sortie claire et structurée, relisez chaque diff, répondez à leurs questions et demandes d'approbation au même endroit, et laissez-les planifier, implémenter et relire le travail des uns des autres.

Vos CLI. Vos comptes. Aucune clé API requise.

[Télécharger pour Mac](https://github.com/kalismeras61/agentdeck/releases/latest) · [Nouveautés](https://github.com/kalismeras61/agentdeck/releases) · [Comment ça marche](#comment-ça-marche) · [Compiler depuis les sources](#compiler-depuis-les-sources)

![Licence : AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Construit avec Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck : une session Claude Code qui demande à exécuter une commande, avec le diff qu'elle a produit à droite](../screenshots/readme/workspace.png)

</div>

## Pourquoi AgentDeck

Les agents de code sont excellents dans un terminal, jusqu'au jour où vous en faites tourner trois sur cinq projets. Les demandes d'approbation défilent hors de l'écran, les diffs se perdent dans les logs, et pour qu'un agent vérifie le travail d'un autre, il faut faire du copier-coller.

AgentDeck garde les agents exactement tels qu'ils sont et leur offre un vrai espace de travail :

- **Il lance les vraies CLI.** `claude`, `codex`, `gemini` démarrent depuis votre propre installation, avec le même PATH, la même configuration et la même connexion que dans le Terminal. Si ça marche dans votre terminal, ça marche ici.
- **Il ne touche jamais à vos identifiants.** Chaque agent se connecte via son propre parcours. AgentDeck ne lit, ne stocke ni n'envoie aucun jeton ; les clés API que vous ajoutez pour vos propres modèles restent dans le trousseau macOS.
- **Tout reste sur votre Mac.** Les sessions sont stockées dans une base SQLite locale. Pas de compte, pas de télémétrie, pas de serveur.

## Ce que vous pouvez faire

### Travaillez avec n'importe quel agent, dans n'importe quel projet

Chaque projet est un dossier ; chaque session est un agent qui y travaille. Changez d'agent avant le premier message, choisissez parmi tous les modèles que propose chaque agent, et changez de modèle en pleine conversation. Lancez une discussion sans projet, déplacez des sessions d'un projet à l'autre, renommez-les, ou importez les sessions que Claude Code, Codex et Gemini CLI ont déjà enregistrées sur votre Mac ; elles reprennent la même conversation.

Choisissez aussi à quel point l'agent réfléchit. Claude Code et Codex disposent d'un réglage d'**effort** à côté du modèle (de low à max, uniquement les niveaux pris en charge par ce modèle). Le menu affiche l'effort que l'agent utilise en ce moment, un changement s'applique immédiatement pendant que Claude Code travaille, et les Réglages définissent l'effort avec lequel démarrent les nouvelles sessions.

### Relisez les changements comme vous relisez une pull request

Chaque fichier modifié par un agent s'affiche avec un diff côte à côte ou unifié, fichier par fichier, avec navigation au clavier et marques « consulté ». Rien n'est commité ni poussé à votre place.

Les captures d'écran et images qu'un agent prend ou consulte (une capture du simulateur, une page rendue, une image qu'il lit) apparaissent directement dans la conversation et dans l'onglet Aperçu ; cliquez sur l'une d'elles pour la voir en taille réelle.

![Revue côte à côte des modifications d'un agent](../screenshots/readme/review.png)

### Approuvez, refusez, ou laissez-le poser la question

Les commandes et modifications qui nécessitent votre accord apparaissent sous forme de cartes qui indiquent ce qui va s'exécuter, où, ce que cela affecte et quel est le niveau de risque. Le mode plan permet à un agent de lire et de proposer avant de modifier quoi que ce soit. Quand un agent a une question (AskUserQuestion de Claude Code, request_user_input de Codex), vous obtenez un vrai formulaire : choisissez une option, plusieurs, ou écrivez votre propre réponse.

![Un agent qui pose deux questions, l'une à choix unique et l'autre à choix multiples](../screenshots/readme/question.png)

### Faites travailler les agents ensemble

- **Workflows : plan → implémentation → revue.** Placez n'importe quel agent à chaque étape, par exemple Claude Code planifie, Codex implémente, Claude Code relit. Chaque étape passe la main dès qu'elle est terminée, les notes de revue retournent directement à l'agent qui implémente, et le tout s'arrête quand le relecteur écrit APPROVED. Les demandes ordinaires sont approuvées pour les agents ; les demandes risquées (suppression, push, déploiement, identifiants, tout ce qui sort du projet) attendent toujours votre accord.
- **Transmettre.** Envoyez n'importe quel plan ou réponse à un autre agent en un clic.
- **Relais.** Deux agents discutent entre eux jusqu'à ce que l'un d'eux dise DONE.

![Un workflow plan, implémentation, revue : Codex a implémenté, Claude Code a relu et approuvé](../screenshots/readme/workflow.png)

### Isolez le travail dans des worktrees git

Démarrez une session dans son propre worktree et sa propre branche, pour qu'un agent ne puisse pas empiéter sur votre copie de travail. Visualisez le graphe des branches dans la vue Git, puis fusionnez le worktree (avec un avertissement clair quand les deux côtés ont modifié les mêmes lignes) ou jetez-le.

![Fusion du worktree d'un agent dans master](../screenshots/readme/worktree.png)

### Toutes les commandes à portée de main

Tapez `/` dans le champ de message pour accéder aux commandes propres à l'agent dans ce dossier (commandes intégrées de Claude Code, vos commandes et skills personnalisés, commandes de Gemini CLI, vos prompts Codex) ainsi qu'à celles d'AgentDeck (`/plan`, `/terminal`, `/workflow`, `/git` …).

![Le menu des commandes slash](../screenshots/readme/slash.png)

### Et tout le reste

- **Configuration de l'agent.** Modifiez `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, les skills, commandes et sous-agents, globaux ou par projet, dans une seule arborescence.
- **Terminal intelligent.** Votre propre shell dans le dossier du projet. Interrogez n'importe quel agent sur la sélection ou sur les 80 dernières lignes, avec un bouton « Expliquer cette erreur » en un clic.
- **Joignez ce que vous voulez.** Déposez des fichiers ou des dossiers, collez une capture d'écran ou choisissez des fichiers. Les images parviennent à l'agent en tant qu'images.
- **Vos propres modèles.** DeepSeek, Kimi, GLM (Z.ai), Ollama, LM Studio, OpenRouter ou n'importe quel endpoint compatible OpenAI ou Anthropic, avec les mêmes approbations, le même mode plan et les mêmes workflows.
- **Connexion là où ça bloque.** Quand un agent a besoin de se connecter, un bouton lance sa propre procédure de connexion dans un terminal intégré, puis renvoie votre message.
- **Mises à jour automatiques.** Les nouvelles versions sont signées et notarisées, et chaque mise à jour est vérifiée avec une clé intégrée à l'application avant son installation.

### Dans votre langue

Toute l'interface est disponible en **English, Türkçe, Deutsch, Русский, Español, Français, Português, 简体中文, 日本語, 한국어 et العربية**, avec une mise en page de droite à gauche pour l'arabe. AgentDeck suit la langue de votre Mac, ou vous en choisissez une dans Réglages → Apparence. Les réponses des agents, le code, les commandes et les noms de fichiers ne sont jamais traduits, et les agents répondent dans la langue que vous utilisez pour leur écrire.

![AgentDeck en turc et en arabe, de droite à gauche](../screenshots/readme/languages.png)

## Léger pour votre Mac

AgentDeck est une application Tauri native : un petit cœur en Rust et le WebKit de macOS, sans moteur de navigateur embarqué.

| | Mesuré sur un Mac (build de release, Moniteur d'activité) |
|---|---|
| Téléchargement | Image disque de 22 Mo (Apple Silicon + Intel dans une seule application) |
| Installé | 50 Mo |
| Mémoire, AgentDeck lui-même | ~31 Mo application + ~207 Mo interface + ~18 Mo processus système ≈ **256 Mo** |
| CPU au repos | **0,1 %**, aucun réveil au repos |
| Démarrage | L'interface charge un bundle principal de 446 Ko ; la revue, le terminal, la galerie et les réglages se chargent à l'ouverture |

Les agents tournent dans leurs propres processus et utilisent leur propre mémoire, environ 140 à 210 Mo chacun pendant qu'ils travaillent (mesuré sous Linux : Claude Code ~188 Mo, Codex ~159 Mo). Pour garder cela sous contrôle, un agent inactif depuis 10 minutes est fermé (sa conversation reprend quand vous écrivez à nouveau), 6 au maximum restent ouverts en même temps, et les longs historiques se chargent par tranches de 80 échanges.

## Agents pris en charge

| Agent | Comment AgentDeck communique avec lui | Connexion |
|---|---|---|
| **Claude Code** | `claude` avec stream-json et son outil de demande d'autorisation | Claude Pro, Max, Team ou Enterprise, ou Anthropic Console |
| **Codex** | `codex app-server`, JSON-RPC sur stdio | Abonnement ChatGPT ou clé API OpenAI |
| **Gemini CLI** | Agent Client Protocol | Compte Google Workspace ou Cloud, ou clé API Gemini |
| **Grok Build** | Agent Client Protocol | Compte xAI ou clé API |
| **Kimi Code** | `kimi acp`, Agent Client Protocol | Abonnement Kimi Code ou clé API Moonshot AI |
| **Antigravity** *(expérimental, désactivé par défaut)* | `agy` stream-json en mode headless | Compte Google ; voir la note dans les Réglages au sujet des conditions de Google |
| **Vos modèles** | L'agent intégré d'AgentDeck, ou au sein de Claude Code pour les API compatibles Anthropic | Clé API dans votre trousseau |

Installez-en au moins un. L'écran de premier lancement indique lesquels sont présents sur votre Mac et installe les autres ou s'y connecte avec la commande officielle de chaque agent, dans le Terminal, où vous voyez ce qu'elle fait.

## Installation

1. Téléchargez le dernier `AgentDeck-<version>.dmg` depuis les [Releases](https://github.com/kalismeras61/agentdeck/releases/latest).
2. Glissez AgentDeck dans Applications et ouvrez-le. L'application est signée et notarisée, elle s'ouvre donc d'un double-clic.
3. Suivez **Commencer** pour configurer un agent, puis ouvrez un dossier de projet.

macOS 12 ou ultérieur, Apple Silicon ou Intel.

## Comment ça marche

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

- Chaque agent tourne comme processus enfant dans le dossier de la session, avec l'environnement de votre shell de connexion.
- Leur sortie est stockée telle quelle, puis normalisée en un flux d'événements unique (messages, appels d'outils, diffs, approbations, questions, plans) et validée avant tout affichage. Tout ce qui est mal formé s'affiche comme une erreur explicite, et n'est jamais exécuté.
- Les demandes d'approbation ne sont tranchées que par vous, ou par un workflow pour les demandes ordinaires ; rien de dangereux n'est approuvé automatiquement.
- Le frontend n'a pas accès au shell : il ne peut appeler que les commandes restreintes d'AgentDeck.

## Compiler depuis les sources

Prérequis : Node 22+, Rust 1.77+ et les outils en ligne de commande Xcode.

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

Les releases (signées, notarisées, universelles, avec mise à jour automatique) sont décrites dans [docs/RELEASING.md](../RELEASING.md) ; ce qui est construit et la façon dont chaque partie est vérifiée, dans [docs/STATUS.md](../STATUS.md) ; les protocoles des agents, dans [docs/PROVIDERS.md](../PROVIDERS.md).

<details>
<summary><b>Clavier</b></summary>

| Action | Touches |
|---|---|
| Nouvelle session | ⌘N |
| Rechercher des sessions | ⌘K |
| Afficher/masquer la barre latérale / le panneau de contexte | ⌘B / ⌘. |
| Onglets de contexte | ⌘1 … ⌘4 |
| Session précédente / suivante | ⌥↑ / ⌥↓ |
| Activer le champ de message | ⌘L |
| Commandes | / au début d'un message |
| Vue enrichie / Vue terminal | ⌘⇧R / ⌘⇧T |
| Toutes les sessions / Revue / Autorisations / Git / Configuration de l'agent / Galerie | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| Réglages | ⌘, |
| Renommer la session | double-clic sur le titre, ou F2 dans la barre latérale |
| Revue : fichier suivant/précédent, modification, marquer comme consulté | J / K, N / P, V |
| Envoyer / nouvelle ligne | ↵ / ⇧↵ |

Les demandes d'autorisation n'ont volontairement aucun raccourci.

</details>

<details>
<summary><b>Structure du projet</b></summary>

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

Stack : Tauri 2 (Rust), React 19, TypeScript (strict), Vite, Tailwind CSS 4, Radix UI, Zustand, Zod, Shiki, Monaco, xterm.js, SQLite.

</details>

## Contribuer

Les issues et les pull requests sont les bienvenues. Lancez les vérifications ci-dessus avant d'ouvrir une PR, et gardez les textes de l'interface courts et simples.

**Traductions.** Les traductions de l'interface ont été réalisées avec l'aide de l'IA et vérifiées par des tests, mais pas encore par des locuteurs natifs : les corrections sont donc très appréciées. Modifiez `src/locales/<language>.json` et lancez `node scripts/i18n-validate.mjs`. Tout nouveau texte d'interface passe par `t()` ; voir [docs/I18N.md](../I18N.md).

AgentDeck est proposé sous licence AGPL et, séparément, sous une licence commerciale. Pour que cela reste possible, il est demandé aux contributeurs de signer un accord de licence de contributeur (CLA) lors de leur première pull request.

## Licence

AgentDeck est un logiciel libre distribué sous la [GNU Affero General Public License v3.0](../../LICENSE). Vous pouvez l'utiliser, l'étudier, le modifier et le partager. Si vous faites fonctionner une version modifiée pour d'autres personnes via un réseau, vous devez leur proposer son code source sous la même licence.

Pour une licence commerciale sans les obligations de l'AGPL (par exemple pour intégrer AgentDeck dans un produit propriétaire), prenez contact via le dépôt.

Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code et Antigravity sont des produits de leurs propriétaires respectifs. AgentDeck n'est ni affilié à eux ni approuvé par eux ; il exécute vos propres copies installées.
