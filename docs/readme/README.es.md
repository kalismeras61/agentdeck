<div align="center">

[English](../../README.md) · [Türkçe](README.tr.md) · [Deutsch](README.de.md) · [Русский](README.ru.md) · **Español** · [Français](README.fr.md) · [Português](README.pt.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [العربية](README.ar.md)

_Esto es una traducción; el texto de referencia es el [README en inglés](../../README.md)._

# AgentDeck

**Un solo espacio de trabajo en tu Mac para todos los agentes de programación con IA que ya usas.**

Ejecuta Claude Code, Codex, Gemini CLI, Kimi Code y tus propios modelos en paralelo. Lee su trabajo como una salida limpia y estructurada, revisa cada diff, responde a sus preguntas y solicitudes de aprobación en un solo lugar, y deja que planifiquen, implementen y revisen el trabajo de los demás.

Tus CLI. Tus cuentas. Sin necesidad de claves de API.

[Descargar para Mac](https://github.com/kalismeras61/agentdeck/releases/latest) · [Novedades](https://github.com/kalismeras61/agentdeck/releases) · [Cómo funciona](#cómo-funciona) · [Compilar desde el código fuente](#compilar-desde-el-código-fuente)

![Licencia: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-8b5cf6)
![macOS 12+](https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%26%20Intel-0B0D14)
![Hecho con Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-24C8DB)

![AgentDeck: una sesión de Claude Code pidiendo permiso para ejecutar un comando, con el diff que generó a la derecha](../screenshots/readme/workspace.png)

</div>

## Por qué AgentDeck

Los agentes de programación funcionan de maravilla en una terminal, hasta que tienes tres de ellos repartidos en cinco proyectos. Las solicitudes de aprobación se pierden al hacer scroll, los diffs quedan enterrados en los logs y conseguir que un agente revise el trabajo de otro implica copiar y pegar.

AgentDeck deja a los agentes exactamente como son y les da un espacio de trabajo en condiciones:

- **Ejecuta las CLI reales.** `claude`, `codex` y `gemini` se inician desde tu propia instalación, con el mismo PATH, la misma configuración y el mismo inicio de sesión que en Terminal. Si funciona en tu terminal, funciona aquí.
- **Nunca toca tus credenciales.** Cada agente inicia sesión con su propio flujo. AgentDeck no lee, guarda ni envía tokens; las claves de API que añades para tus propios modelos se quedan en el llavero de macOS.
- **Todo se queda en tu Mac.** Las sesiones se guardan en una base de datos SQLite local. Sin cuenta, sin telemetría, sin servidor.

## Qué puedes hacer

### Trabaja con cualquier agente, en cualquier proyecto

Cada proyecto es una carpeta; cada sesión es un agente trabajando en ella. Cambia de agente antes del primer mensaje, elige entre todos los modelos que ofrece cada agente y cambia de modelo a mitad de la conversación. Empieza un chat sin proyecto, mueve sesiones entre proyectos, cámbiales el nombre o importa las sesiones que Claude Code, Codex y Gemini CLI ya guardaron en tu Mac; retoman la misma conversación.

También puedes elegir cuánto se esfuerza el agente al pensar. Claude Code y Codex tienen un ajuste de **esfuerzo** junto al modelo (de bajo a máximo, solo los niveles que admite ese modelo). El menú muestra el esfuerzo que el agente está usando en ese momento, un cambio se aplica de inmediato mientras Claude Code trabaja, y en Ajustes defines el esfuerzo con el que empiezan las sesiones nuevas.

### Revisa lo que cambió, como revisas un pull request

Cada archivo que toca un agente aparece con un diff en paralelo o unificado, archivo por archivo, con navegación por teclado y marcas de «visto». No se hace commit ni push de nada por ti.

Las capturas e imágenes que un agente toma o consulta (una captura del simulador, una página renderizada, una imagen que lee) aparecen directamente en la conversación y en la pestaña Vista previa; haz clic en una para verla a tamaño completo.

![Revisando los cambios de un agente en paralelo](../screenshots/readme/review.png)

### Aprueba, deniega o deja que pregunte

Los comandos y las ediciones que necesitan tu visto bueno aparecen como tarjetas que indican qué se va a ejecutar, dónde, a qué afecta y qué riesgo tiene. El modo plan permite que un agente lea y proponga antes de cambiar nada. Cuando un agente tiene una pregunta (AskUserQuestion de Claude Code, request_user_input de Codex), recibes un formulario de verdad: elige una opción, varias o escribe la tuya.

![Un agente haciendo dos preguntas, una de opción única y otra de opción múltiple](../screenshots/readme/question.png)

### Deja que los agentes trabajen juntos

- **Flujos de trabajo: planificar → implementar → revisar.** Asigna cualquier agente a cada etapa; por ejemplo, Claude Code planifica, Codex implementa y Claude Code revisa. Cada etapa pasa el testigo en cuanto termina, las notas de revisión vuelven directamente a quien implementa y el flujo se detiene cuando el revisor escribe APPROVED. Las solicitudes normales se aprueban en nombre de los agentes; las arriesgadas (borrar, hacer push, desplegar, credenciales, cualquier cosa fuera del proyecto) siguen esperándote.
- **Pasar.** Envía cualquier plan o respuesta a otro agente con un clic.
- **Relevo.** Dos agentes hablan entre sí hasta que uno dice DONE.

![Un flujo de planificar, implementar y revisar: Codex implementó, Claude Code revisó y aprobó](../screenshots/readme/workflow.png)

### Bots que siguen trabajando

Un bot es un agente que se queda. Dale un nombre, un trabajo, instrucciones y un agente y un modelo (Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code, Antigravity o tus propios modelos), y luego encárgale tareas. Las va resolviendo en segundo plano mientras tú haces otra cosa.

- **Un lugar propio.** Cada bot tiene su propia carpeta (`~/AgentDeck/Bots/<name>`) y trabaja allí salvo que una tarea indique un proyecto. Puedes dirigir cualquier tarea a cualquier proyecto; el bot mantiene una conversación aparte para cada lugar, así que recuerda qué hizo y dónde.
- **Una cola de tareas.** Añade tareas mientras el bot está ocupado; las ejecuta de una en una. Pausa, reanuda, cancela o reintenta, y cada tarea terminada conserva el informe del bot como resultado.
- **Memoria.** Cada bot tiene un breve archivo de memoria en su carpeta. Lo lee al empezar cada tarea y apunta lo que vale la pena recordar: convenciones, decisiones, dónde está cada cosa, qué prefieres. Puedes leerlo y editarlo en la página del bot.
- **Programaciones.** «Los días laborables a las 9:00: revisa los pull requests fusionados ayer», «cada 6 horas: busca tests inestables». Cada ejecución entra en la cola como cualquier tarea.
- **Un bot gestor.** Los bots pueden pasar partes de una tarea a otros bots y esperar sus resultados, mediante las herramientas propias de AgentDeck (`list_bots`, `delegate_task`, `wait_for_tasks`). Un bot principal puede repartir el trabajo entre un revisor, un tester y un redactor, y luego juntar sus informes.
- **Desatendido, pero no sin control.** Mientras un bot trabaja en una tarea, las solicitudes normales dentro de su carpeta y del proyecto de la tarea se aprueban en su nombre. Todo lo arriesgado (borrar, hacer push, desplegar, credenciales, cualquier cosa en otro lugar) y cada pregunta te esperan a ti, y el bot muestra «Te necesita» en la barra lateral.
- **Telegram.** Conecta un bot de Telegram propio (un token de @BotFather, guardado en tu llavero) y vincula tu chat con un código. Después escribe una tarea desde el móvil y la respuesta del bot te llega allí; empieza con el nombre de un bot («Reviewer: …») para elegirlo. Cuando un bot te necesita, el paso arriesgado, la pregunta o el plan llegan con botones: permitir, denegar, elegir una respuesta. AgentDeck pide los mensajes a Telegram por su cuenta, así que nada en tu Mac queda abierto a internet, y solo se responde a los chats que vinculaste.

Próximamente: conexiones (correo, calendario, un navegador, GitHub y más) para que los bots hagan algo más que programar, y más formas de ponerlos en marcha (que cambie una página web, que llegue un correo, una etiqueta de GitHub).

![Un bot avanzando por su cola, con sus programaciones y su memoria a una pestaña de distancia](../screenshots/readme/bots.png)

### Aísla el trabajo en worktrees de git

Inicia una sesión en su propio worktree y rama, para que un agente no pueda pisar tu copia de trabajo. Consulta el grafo de ramas en la vista Git y luego fusiona el worktree de vuelta (con un aviso claro cuando ambos lados cambiaron las mismas líneas) o descártalo.

![Fusionando el worktree de un agente de vuelta en master](../screenshots/readme/worktree.png)

### Todos los comandos a mano

Escribe `/` en el cuadro de mensaje para ver los comandos propios del agente en esa carpeta (los integrados de Claude Code, tus comandos y habilidades personalizados, los comandos de Gemini CLI, tus prompts de Codex) además de los de AgentDeck (`/plan`, `/terminal`, `/workflow`, `/git` …).

![El menú de comandos con barra](../screenshots/readme/slash.png)

### Y mucho más

- **Configuración de agentes.** Edita `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, habilidades, comandos y subagentes, globales y por proyecto, en un solo árbol.
- **Terminal inteligente.** Tu propio shell en la carpeta del proyecto. Pregunta a cualquier agente sobre la selección o las últimas 80 líneas, con un «Explicar este error» a un clic.
- **Adjunta lo que sea.** Suelta archivos o carpetas, pega una captura o elige archivos. Las imágenes llegan al agente como imágenes.
- **Tus propios modelos.** DeepSeek, Kimi, GLM (Z.ai), Ollama, LM Studio, OpenRouter o cualquier endpoint compatible con OpenAI o Anthropic, con las mismas aprobaciones, modo plan y flujos de trabajo.
- **Límites del plan de un vistazo.** Bajo el cuadro de mensaje, las sesiones de Claude Code y Codex muestran cuánto has usado de tus límites de 5 horas y semanal, y cuándo se restablecen.
- **Inicia sesión donde falla.** Cuando un agente necesita iniciar sesión, un botón ejecuta su propio inicio de sesión en una terminal integrada y vuelve a enviar tu mensaje.
- **Se actualiza solo.** Las versiones nuevas están firmadas y notarizadas, y cada actualización se verifica con una clave integrada en la app antes de instalarse.

### En tu idioma

Toda la interfaz está disponible en **English, Türkçe, Deutsch, Русский, Español, Français, Português, 简体中文, 日本語, 한국어 y العربية**, con diseño de derecha a izquierda para el árabe. AgentDeck sigue el idioma de tu Mac, o puedes elegir uno en Ajustes → Apariencia. Las respuestas de los agentes, el código, los comandos y los nombres de archivo nunca se traducen, y los agentes responden en el idioma en que les escribas.

![AgentDeck en turco y en árabe, de derecha a izquierda](../screenshots/readme/languages.png)

## Ligero para tu Mac

AgentDeck es una app nativa de Tauri: un pequeño núcleo en Rust y el propio WebKit de macOS, sin motor de navegador incluido.

| | Medido en un Mac (build de release, Monitor de Actividad) |
|---|---|
| Descarga | Imagen de disco de 22 MB (Apple Silicon + Intel en una sola app) |
| Instalado | 50 MB |
| Memoria, solo AgentDeck | ~31 MB app + ~207 MB interfaz + ~18 MB procesos auxiliares del sistema ≈ **256 MB** |
| CPU en reposo | **0,1 %**, sin activaciones en reposo |
| Arranque | La interfaz carga un bundle principal de 446 KB; la revisión, la terminal, la galería y los ajustes se cargan al abrirlos |

Los agentes se ejecutan como procesos propios y usan su propia memoria, unos 140–210 MB cada uno mientras trabajan (medido en Linux: Claude Code ~188 MB, Codex ~159 MB). Para mantenerlo bajo control, un agente que lleva 10 minutos inactivo se cierra (su conversación continúa cuando vuelves a escribir), como máximo hay 6 abiertos a la vez y los historiales largos se cargan de 80 turnos en 80.

## Agentes compatibles

| Agente | Cómo se comunica AgentDeck con él | Inicio de sesión |
|---|---|---|
| **Claude Code** | `claude` con stream-json y su herramienta de solicitud de permisos | Claude Pro, Max, Team o Enterprise, o Anthropic Console |
| **Codex** | `codex app-server`, JSON-RPC sobre stdio | Plan de ChatGPT o clave de API de OpenAI |
| **Gemini CLI** | Agent Client Protocol | Cuenta de Google Workspace o Cloud, o una clave de API de Gemini |
| **Grok Build** | Agent Client Protocol | Cuenta de xAI o clave de API |
| **Kimi Code** | `kimi acp`, Agent Client Protocol | Plan de Kimi Code o clave de API de Moonshot AI |
| **Antigravity** *(experimental, desactivado por defecto)* | `agy` en modo headless con stream-json | Cuenta de Google; consulta la nota en Ajustes sobre los términos de Google |
| **Tus modelos** | El agente integrado de AgentDeck, o dentro de Claude Code para API compatibles con Anthropic | Clave de API en tu llavero |

Instala al menos uno. La pantalla de primer inicio muestra cuáles tienes en tu Mac e instala el resto, o inicia sesión en ellos, con el comando oficial de cada agente, en Terminal, donde puedes ver lo que hace.

## Instalación

1. Descarga el último `AgentDeck-<version>.dmg` desde [Releases](https://github.com/kalismeras61/agentdeck/releases/latest).
2. Arrastra AgentDeck a Aplicaciones y ábrelo. Está firmado y notarizado, así que se abre con un doble clic.
3. Sigue **Empezar** para configurar un agente y luego abre una carpeta de proyecto.

macOS 12 o posterior, Apple Silicon o Intel.

## Cómo funciona

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

- Cada agente se ejecuta como un proceso hijo en la carpeta de la sesión, con el entorno de tu shell de inicio de sesión.
- Su salida se guarda tal cual y luego se normaliza en un único flujo de eventos (mensajes, llamadas a herramientas, diffs, aprobaciones, preguntas, planes) que se valida antes de mostrar nada. Todo lo que esté mal formado se muestra como un error claro y nunca se ejecuta.
- Las aprobaciones solo las respondes tú, o un flujo de trabajo en el caso de solicitudes normales; nada peligroso se aprueba automáticamente.
- El frontend no tiene acceso al shell: solo puede llamar a los comandos propios y acotados de AgentDeck.

## Compilar desde el código fuente

Requisitos: Node 22+, Rust 1.77+ y las herramientas de línea de comandos de Xcode.

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

Las releases (firmadas, notarizadas, universales y con actualización automática) se describen en [docs/RELEASING.md](../RELEASING.md); qué está construido y cómo se verifica cada parte, en [docs/STATUS.md](../STATUS.md); los protocolos de los agentes, en [docs/PROVIDERS.md](../PROVIDERS.md).

<details>
<summary><b>Teclado</b></summary>

| Acción | Teclas |
|---|---|
| Nueva sesión | ⌘N |
| Buscar sesiones | ⌘K |
| Mostrar u ocultar la barra lateral / el panel de contexto | ⌘B / ⌘. |
| Pestañas de contexto | ⌘1 … ⌘4 |
| Sesión anterior / siguiente | ⌥↑ / ⌥↓ |
| Ir al cuadro de mensaje | ⌘L |
| Comandos | / al inicio de un mensaje |
| Vista enriquecida / Vista de terminal | ⌘⇧R / ⌘⇧T |
| Todas las sesiones / Revisar / Permisos / Git / Configuración de agentes / Galería | ⌘⇧S / ⌘⇧D / ⌘⇧A / ⌘⇧H / ⌘⇧E / ⌘⇧G |
| Ajustes | ⌘, |
| Cambiar el nombre de la sesión | doble clic en el título, o F2 en la barra lateral |
| Revisar: archivo siguiente/anterior, cambio, marcar como visto | J / K, N / P, V |
| Enviar / nueva línea | ↵ / ⇧↵ |

Las solicitudes de permiso no tienen atajo a propósito.

</details>

<details>
<summary><b>Estructura del proyecto</b></summary>

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

## Contribuir

Los issues y pull requests son bienvenidos. Ejecuta las comprobaciones de arriba antes de abrir un PR y mantén los textos de la interfaz breves y sencillos.

**Traducciones.** Las traducciones de la interfaz se hicieron con ayuda de IA y están verificadas por tests, pero todavía no por hablantes nativos, así que las correcciones son muy bienvenidas: edita `src/locales/<language>.json` y ejecuta `node scripts/i18n-validate.mjs`. Los textos nuevos de la interfaz pasan por `t()`; consulta [docs/I18N.md](../I18N.md).

AgentDeck se ofrece bajo la AGPL y, por separado, bajo una licencia comercial. Para que esto siga siendo posible, se pide a quienes contribuyen que firmen un acuerdo de licencia de contribuidor (CLA) en su primer pull request.

## Licencia

AgentDeck es software libre bajo la [GNU Affero General Public License v3.0](../../LICENSE). Puedes usarlo, estudiarlo, modificarlo y compartirlo. Si ejecutas una versión modificada para otras personas a través de una red, debes ofrecerles su código fuente bajo la misma licencia.

Para obtener una licencia comercial sin las obligaciones de la AGPL (por ejemplo, para distribuir AgentDeck dentro de un producto cerrado), ponte en contacto a través del repositorio.

Claude Code, Codex, Gemini CLI, Grok Build, Kimi Code y Antigravity son productos de sus respectivos propietarios. AgentDeck no está afiliado a ellos ni cuenta con su respaldo; ejecuta las copias que tú mismo has instalado.
