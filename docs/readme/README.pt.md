<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · [Español](README.es.md) · [Français](README.fr.md) · **Português** · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [العربية](README.ar.md)

_Esta é uma tradução; o texto de referência é o [README em inglês](../../README.md)._

# AgentDeck

**Um único workspace no Mac para todos os agentes de código com IA que você já usa.**

Rode Claude Code, Codex, Gemini CLI, Kimi Code e seus próprios modelos lado a lado. Acompanhe o trabalho deles em uma saída limpa e estruturada, revise cada diff, responda às perguntas e aprovações em um só lugar e deixe que eles planejem, implementem e revisem o trabalho uns dos outros.

Suas CLIs. Suas contas. Sem precisar de chave de API.

[Baixar para Mac](https://github.com/kalismeras61/agentdeck/releases/latest) · [Novidades](https://github.com/kalismeras61/agentdeck/releases) · [Como funciona](#como-funciona) · [Compilar a partir do código-fonte](#compilar-a-partir-do-código-fonte)

![Licença: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Feito com Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck: uma sessão do Claude Code pedindo para executar um comando, com o diff que ela gerou à direita](../screenshots/readme/workspace.png)

</div>

## Por que o AgentDeck

Agentes de código funcionam muito bem no terminal, até você rodar três deles em cinco projetos. As aprovações somem rolando a tela, os diffs ficam enterrados nos logs, e fazer um agente conferir o trabalho de outro vira copiar e colar.

O AgentDeck mantém os agentes exatamente como são e dá a eles um workspace de verdade:

- **Ele roda as CLIs de verdade.** `claude`, `codex`, `gemini` iniciam a partir da sua própria instalação, com o mesmo PATH, a mesma configuração e o mesmo login do Terminal. Se funciona no seu terminal, funciona aqui.
- **Ele nunca mexe nas suas credenciais.** Cada agente faz login pelo próprio fluxo. O AgentDeck não lê, não armazena nem envia tokens; as chaves de API que você adiciona para seus próprios modelos ficam no Keychain do macOS.
- **Tudo fica no seu Mac.** As sessões ficam em um banco SQLite local. Sem conta, sem telemetria, sem servidor.

## O que você pode fazer

### Trabalhe com qualquer agente, em qualquer projeto

Cada projeto é uma pasta; cada sessão é um agente trabalhando nela. Troque de agente antes da primeira mensagem, escolha entre todos os modelos que cada agente oferece e troque de modelo no meio da conversa. Comece um chat sem projeto, mova sessões entre projetos, renomeie-as ou importe as sessões que o Claude Code, o Codex e o Gemini CLI já salvaram no seu Mac; elas continuam a mesma conversa.

Escolha também o quanto o agente deve pensar. Claude Code e Codex têm um ajuste de **esforço** ao lado do modelo (de low a max, só os níveis que aquele modelo suporta). O menu mostra o esforço que o agente está usando no momento, uma mudança vale na hora enquanto o Claude Code trabalha, e em Ajustes você define o esforço com que as novas sessões começam.

### Revise o que mudou como você revisa um pull request

Cada arquivo que um agente toca aparece com um diff lado a lado ou unificado, arquivo por arquivo, com navegação pelo teclado e marcação de "visto". Nada é commitado nem enviado com push por você.

Capturas de tela e imagens que um agente gera ou consulta (uma captura do simulador, uma página renderizada, uma imagem que ele lê) aparecem direto na conversa e na aba Prévia; clique em uma para vê-la em tamanho real.

![Revisando as alterações de um agente lado a lado](../screenshots/readme/review.png)

### Aprove, negue ou deixe o agente perguntar

Comandos e edições que precisam do seu OK aparecem como cards que dizem o que vai rodar, onde, o que é afetado e qual o nível de risco. O modo de planejamento deixa o agente ler e propor antes de alterar qualquer coisa. Quando um agente tem uma pergunta (AskUserQuestion do Claude Code, request_user_input do Codex), você recebe um formulário de verdade: escolha uma opção, várias, ou escreva a sua.

![Um agente fazendo duas perguntas, uma de escolha única e outra de múltipla escolha](../screenshots/readme/question.png)

### Deixe os agentes trabalharem juntos

- **Fluxos de trabalho: plano → implementação → revisão.** Coloque qualquer agente em cada etapa, por exemplo: o Claude Code planeja, o Codex implementa, o Claude Code revisa. Cada etapa passa adiante assim que termina, as notas de revisão voltam direto para quem implementa, e o fluxo para quando o revisor escreve APPROVED. Pedidos comuns são aprovados para os agentes; os arriscados (apagar, fazer push, fazer deploy, credenciais, qualquer coisa fora do projeto) continuam esperando por você.
- **Transferir.** Envie qualquer plano ou resposta para outro agente com um clique.
- **Revezamento.** Dois agentes conversam entre si até que um diga DONE.

![Um fluxo de trabalho de plano, implementação e revisão: o Codex implementou, o Claude Code revisou e aprovou](../screenshots/readme/workflow.png)

### Bots que continuam trabalhando

Um bot é um agente que fica. Dê a ele um nome, uma função, instruções, um agente e um modelo (Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity ou seus próprios modelos) e uma pasta, e depois passe tarefas para ele. Ele as resolve em segundo plano enquanto você faz outra coisa.

- **Uma fila de tarefas.** Adicione tarefas enquanto o bot está ocupado; ele executa uma de cada vez. Pause, retome, cancele ou tente de novo, e cada tarefa concluída guarda o relatório do bot como resultado.
- **Memória.** Cada bot mantém um arquivo de memória curto (fora do git). Ele lê esse arquivo no início de cada tarefa e anota o que vale a pena lembrar: convenções, decisões, onde as coisas ficam, o que você prefere. Você pode ler e editar a memória na página do bot.
- **Sem supervisão, mas não sem controle.** Enquanto um bot trabalha em uma tarefa, pedidos comuns são aprovados para ele. Qualquer coisa arriscada (apagar, fazer push, fazer deploy, credenciais, qualquer coisa fora da pasta dele) e toda pergunta esperam por você, e o bot mostra "Precisa de você" na barra lateral.
- **Uma página só dele.** A conversa do bot com diffs e aprovações como em qualquer sessão, uma caixa para dar uma tarefa a ele ou enviar uma mensagem simples, e a fila e a memória dele lado a lado.

Em breve: um workspace próprio para cada bot (com cada tarefa podendo indicar um projeto), agendamentos ("dias úteis às 9:00"), um bot gerente que distribui trabalho para outros bots e reúne os resultados, e acesso aos seus bots pelo celular ou pelo Telegram.

![Um bot trabalhando na fila dele, com a memória a uma aba de distância](../screenshots/readme/bots.png)

### Isole o trabalho em worktrees do git

Inicie uma sessão no próprio worktree e branch, para que o agente não atrapalhe sua cópia de trabalho. Veja o grafo de branches na visualização Git e depois faça merge do worktree (com um aviso claro quando os dois lados alteraram as mesmas linhas) ou descarte-o.

![Fazendo merge do worktree de um agente de volta no master](../screenshots/readme/worktree.png)

### Todos os comandos na ponta dos dedos

Digite `/` na caixa de mensagem para ver os comandos do próprio agente naquela pasta (os comandos nativos do Claude Code, seus comandos e skills personalizados, os comandos do Gemini CLI, seus prompts do Codex), além dos do AgentDeck (`/plan`, `/terminal`, `/workflow`, `/git` …).

![O menu de comandos de barra](../screenshots/readme/slash.png)

### E tem mais

- **Configuração dos agentes.** Edite `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, skills, comandos e subagentes, globais e por projeto, em uma única árvore.
- **Terminal inteligente.** Seu próprio shell na pasta do projeto. Pergunte a qualquer agente sobre a seleção ou as últimas 80 linhas, com um botão "Explique este erro" a um clique.
- **Anexe qualquer coisa.** Solte arquivos ou pastas, cole uma captura de tela ou escolha arquivos. Imagens chegam ao agente como imagens.
- **Seus próprios modelos.** DeepSeek, Kimi, GLM (Z.ai), Ollama, LM Studio, OpenRouter ou qualquer endpoint compatível com OpenAI ou Anthropic, com as mesmas aprovações, o mesmo modo de planejamento e os mesmos fluxos de trabalho.
- **Limites do plano à vista.** Abaixo da caixa de mensagem, as sessões do Claude Code e do Codex mostram quanto você já usou dos limites de 5 horas e semanal e quando eles reiniciam.
- **Login onde ele falha.** Quando um agente precisa de login, um botão executa o login dele em um terminal integrado e envia sua mensagem de novo.
- **Atualiza sozinho.** As novas versões são assinadas e notarizadas, e cada atualização é verificada com uma chave embutida no app antes de ser instalada.

### No seu idioma

Toda a interface está disponível em **English, Türkçe, Deutsch, Русский, Español, Français, Português, 简体中文, 日本語, 한국어 e العربية**, com layout da direita para a esquerda para o árabe. O AgentDeck segue o idioma do seu Mac, ou você escolhe um em Ajustes → Aparência. Respostas dos agentes, código, comandos e nomes de arquivos nunca são traduzidos, e os agentes respondem no idioma em que você escrever para eles.

![AgentDeck em turco e em árabe, da direita para a esquerda](../screenshots/readme/languages.png)

## Leve para o seu Mac

O AgentDeck é um app Tauri nativo: um pequeno núcleo em Rust e o próprio WebKit do macOS, sem motor de navegador embutido.

| | Medido em um Mac (build de release, Monitor de Atividade) |
|---|---|
| Download | Imagem de disco de 22 MB (Apple Silicon + Intel em um só app) |
| Instalado | 50 MB |
| Memória, o próprio AgentDeck | ~31 MB app + ~207 MB interface + ~18 MB processos auxiliares do sistema ≈ **256 MB** |
| CPU em repouso | **0,1 %**, sem despertares em repouso |
| Inicialização | A interface carrega um bundle principal de 446 KB; revisão, terminal, galeria e ajustes carregam quando abertos |

Os agentes rodam como processos próprios e usam a própria memória, cerca de 140–210 MB cada um enquanto trabalham (medido no Linux: Claude Code ~188 MB, Codex ~159 MB). Para manter isso sob controle, um agente ocioso há 10 minutos é fechado (a conversa continua quando você escrever de novo), no máximo 6 ficam abertos ao mesmo tempo, e históricos longos carregam 80 turnos por vez.

## Agentes compatíveis

| Agente | Como o AgentDeck se comunica com ele | Login |
|---|---|---|
| **Claude Code** | `claude` com stream-json e a ferramenta de pedido de permissão dele | Claude Pro, Max, Team ou Enterprise, ou Anthropic Console |
| **Codex** | `codex app-server`, JSON-RPC via stdio | Plano do ChatGPT ou chave de API da OpenAI |
| **Gemini CLI** | Agent Client Protocol | Conta do Google Workspace ou do Cloud, ou uma chave de API do Gemini |
| **Grok Build** | Agent Client Protocol | Conta xAI ou chave de API |
| **Kimi Code** | `kimi acp`, Agent Client Protocol | Plano do Kimi Code ou chave de API da Moonshot AI |
| **Antigravity** *(experimental, desativado por padrão)* | `agy` headless com stream-json | Conta Google; veja a observação em Ajustes sobre os termos do Google |
| **Seus modelos** | O agente integrado do AgentDeck, ou dentro do Claude Code para APIs compatíveis com Anthropic | Chave de API no seu Keychain |

Instale pelo menos um. A tela de primeira execução mostra quais estão no seu Mac e instala os demais ou faz login neles com o comando oficial de cada agente, no Terminal, onde você vê o que ele faz.

## Instalação

1. Baixe o `AgentDeck-<version>.dmg` mais recente em [Releases](https://github.com/kalismeras61/agentdeck/releases/latest).
2. Arraste o AgentDeck para Aplicativos e abra. Ele é assinado e notarizado, então abre com um clique duplo.
3. Siga o **Começar** para configurar um agente e depois abra uma pasta de projeto.

macOS 12 ou posterior, Apple Silicon ou Intel.

## Como funciona

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

- Cada agente roda como processo filho na pasta da sessão, com o ambiente do seu shell de login.
- A saída deles é armazenada como veio e depois normalizada em um único fluxo de eventos (mensagens, chamadas de ferramentas, diffs, aprovações, perguntas, planos) e validada antes de qualquer coisa ser renderizada. Qualquer coisa malformada aparece como um erro claro, nunca é executada.
- As aprovações são respondidas só por você, ou por um fluxo de trabalho no caso de pedidos comuns; nada perigoso é aprovado automaticamente.
- O frontend não tem acesso ao shell: ele só pode chamar os comandos restritos do próprio AgentDeck.

## Compilar a partir do código-fonte

Requisitos: Node 22+, Rust 1.77+ e as ferramentas de linha de comando do Xcode.

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

As releases (assinadas, notarizadas, universais, com atualização automática) estão descritas em [docs/RELEASING.md](../RELEASING.md); o que está pronto e como cada parte é verificada, em [docs/STATUS.md](../STATUS.md); os protocolos dos agentes, em [docs/PROVIDERS.md](../PROVIDERS.md).

<details>
<summary><b>Teclado</b></summary>

| Ação | Teclas |
|---|---|
| Nova sessão | ⌘N |
| Buscar sessões | ⌘K |
| Alternar barra lateral / painel de contexto | ⌘B / ⌘. |
| Abas de contexto | ⌘1 … ⌘4 |
| Sessão anterior / próxima | ⌥↑ / ⌥↓ |
| Focar a caixa de mensagem | ⌘L |
| Comandos | / no início de uma mensagem |
| Visualização rica / de terminal | ⌘⇧R / ⌘⇧T |
| Todas as sessões / Revisão / Permissões / Git / Configuração dos agentes / Galeria | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| Ajustes | ⌘, |
| Renomear sessão | clique duplo no título, ou F2 na barra lateral |
| Revisão: próximo/anterior arquivo, alteração, marcar como visto | J / K, N / P, V |
| Enviar / nova linha | ↵ / ⇧↵ |

Os pedidos de permissão não têm atalho, de propósito.

</details>

<details>
<summary><b>Estrutura do projeto</b></summary>

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

Stack: Tauri 2 (Rust), React 19, TypeScript (strict), Vite, Tailwind CSS 4, Radix UI, Zustand, Zod, Shiki, Monaco, xterm.js, SQLite.

</details>

## Como contribuir

Issues e pull requests são bem-vindos. Rode as verificações acima antes de abrir um PR e mantenha os textos da interface curtos e simples.

**Traduções.** As traduções da interface foram feitas com ajuda de IA e verificadas por testes, mas ainda não por falantes nativos, então correções são muito bem-vindas: edite `src/locales/<language>.json` e rode `node scripts/i18n-validate.mjs`. Todo texto novo da interface passa por `t()`; veja [docs/I18N.md](../I18N.md).

O AgentDeck é oferecido sob a AGPL e, separadamente, sob uma licença comercial. Para que isso continue possível, pedimos que quem contribui assine um acordo de licença de contribuidor (CLA) no primeiro pull request.

## Licença

O AgentDeck é software livre sob a [GNU Affero General Public License v3.0](../../LICENSE). Você pode usá-lo, estudá-lo, modificá-lo e compartilhá-lo. Se você executar uma versão modificada para outras pessoas por meio de uma rede, deve oferecer a elas o código-fonte dessa versão sob a mesma licença.

Para uma licença comercial sem as obrigações da AGPL (por exemplo, para distribuir o AgentDeck dentro de um produto fechado), entre em contato pelo repositório.

Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code e Antigravity são produtos de seus respectivos donos. O AgentDeck não é afiliado a eles nem endossado por eles; ele executa as cópias que você mesmo instalou.
