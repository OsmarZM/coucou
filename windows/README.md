<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" alt="Coucou icon">

# Coucou for Windows

**Mochi doesn't get a notch on a PC — so it lives at the top of your screen instead.**

Approve permissions, watch your session work, drop documents, chat through an installed CLI, keep an eye on your services — without leaving what you're doing.

![Windows 10/11](https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows)
![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
![Rust](https://img.shields.io/badge/Rust-backend-000?logo=rust)
![License: MIT](https://img.shields.io/badge/license-MIT-green)

</div>

<img src="screenshots/greeting.png" width="640" alt="Mochi waving hello at launch">

---

## Agente pessoal — versão local 0.1.2

Para preparar o computador, instalar/autenticar as quatro CLIs, gerar o pacote e começar a usar, siga o [README de instalação passo a passo em português](../README-WINDOWS.md).

O chat pessoal agora usa uma conversa contínua e escolha pelo personagem animado. Histórico, preferências e anexos são compartilhados no contexto pessoal; IDs de sessão e permissões de cada CLI permanecem separados. Não há botão para ativar aprendizado nem seletores principais de agente/conversa. Projeto e retomada manual ficam em controles avançados.

O chat está habilitado para Codex e Claude. Gemini e Copilot podem ser instalados, autenticados e monitorados, mas o chat fica indisponível em todos os modos nesta 0.1.2 até a qualificação do isolamento de hooks e MCP. Selecionar um projeto não habilita esses envios.

Leia o [guia em português](../docs/AGENTE-PESSOAL-WINDOWS.md), a [validação desta versão](../docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) e a [política de mensagens entre chats](../docs/CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md). O pacote local versionado é distinto de instalação, publicação e homologação da janela nativa.

## Install

This checkout generates a **local 0.1.2 installer** with `npm run pack`. Follow the
[Portuguese installation guide](../README-WINDOWS.md) to install the CLIs, sign in,
build the package and install for the current Windows user.

The original upstream download notice concerned an unsigned installer and a
Defender report. Its current release and signing status were not verified for
this local delivery. A locally generated package does not establish publication,
signature status or installed-app qualification; see the
[current validation report](../docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md).

## Using it

The images below show the original Windows UI. The current personal chat uses
animated provider characters and one continuous timeline; consult the current
validation report for evidence of that flow.

<img src="screenshots/compact.png" width="292" alt="The compact island, with the integration pills as mini Mochis">
<img src="screenshots/overview.png" width="640" alt="The overview: the focused integration on the left, the other pills on the right">
<img src="screenshots/approval.png" width="640" alt="A Claude Code permission request, with Deny and Allow">
<img src="screenshots/chat.png" width="640" alt="Chatting with Claude from the island">
<img src="screenshots/drop.png" width="640" alt="Mochi turned into a box, waiting for a file">

| What you do | What happens |
|---|---|
| Move the mouse to the very top-centre of the screen | Reveals the island when it was explicitly hidden |
| Click the small island | It opens |
| Click Mochi | It gets annoyed. Three times in a row and it goes dizzy |
| Rest the pointer on Mochi for two seconds | Hearts |
| Drag a file onto the island | Mochi turns into a box, swallows it, then offers to answer questions about it |
| `Esc` | Closes the island |
| Tray icon | Open, Settings…, Pause, Quit |

Everything else happens on its own: a Claude Code permission request opens the
island with **Deny / Allow**, a finished session shows what it did, and
your integrations sit in the coloured pills next to Mochi.

## Claude Code

The Windows implementation also supports local **Codex CLI, Gemini CLI and GitHub
Copilot CLI** monitoring. Settings contains a separate preview/install/remove flow
for each CLI. Monitoring groups keep independent session selectors; they are
separate from the continuous personal chat. Hook permission approval is available
for Claude's supported requests. The personal Codex chat has its own permission
cards for Coucou-mediated tools.
See [the multiagent guide](../docs/MULTIAGENTE-WINDOWS.md) for configuration paths,
activation, native fallbacks and the validation matrix. Desktop/editor surfaces
require their own qualification.

<img src="screenshots/settings.png" width="562" alt="The settings window">

Open **Settings… → Claude Code → Install hooks…**. You get the exact diff of what
will change in `%USERPROFILE%\.claude\settings.json`, the path of the dated backup
that will be taken, and nothing is written until you click. Your own hooks are
never touched, and uninstalling removes only Coucou's entries.

The relay is a tiny executable, `coucou-hook.exe`, copied to
`%LOCALAPPDATA%\Coucou\bin\` at launch. It uses bounded connection and approval
deadlines. If the app is unavailable or nobody answers in time, the hook falls
back to the CLI's native permission flow. See the multiagent guide for the
provider-specific behavior and limits.

It works from any terminal — Windows Terminal, PowerShell, VS Code, Git Bash.

## Chat and keys

**Settings… → Claude** takes your Anthropic API key. Keys live in the **Windows
Credential Manager**, never on disk and never in the interface — the island can
only ask whether a key exists. Same for every integration key.

No Coucou telemetry. Configured service integrations and authenticated CLI turns
can make network requests to their providers; each CLI has its own account and
data policies.

## Build it yourself

You need [Rust](https://rustup.rs), [Node 22+](https://nodejs.org), and the
**MSVC build tools** (Visual Studio Build Tools with "Desktop development with
C++"). Ensure the WebView2 Runtime is available. The
[installation guide](../README-WINDOWS.md) lists the complete Windows prerequisites.

```powershell
cd windows
npm ci
npm run tauri dev      # live-reloading development build
npm run pack           # builds the installer and drops it in windows/release/
```

`npm run dev` alone serves the front end in an ordinary browser, which is enough
to work on the island's looks. It also serves `dev/upload-preview.html`, which
replays the whole file-drop choreography on a loop — the one part of the UI that
otherwise needs a real drag from Explorer to see. Neither page ships in the app.

`npm run pack` leaves two files in `windows/release/`, the same names the release
workflow publishes:

```
Coucou-Windows-X.Y.Z-setup.exe    the versioned installer
Coucou-Windows-setup.exe          the same file under the rolling name
```

Installing is optional — `target/release/coucou.exe` runs on its own. There is no
window in the taskbar and no console: the island at the top of the screen and the
Mochi in the notification area are the whole app, and Quit lives in its menu.

The 28 sounds are the macOS app's own files; they are never duplicated in this
folder. The path is declared once, in `SOUNDS_DIR` at the top of
`vite.config.ts` — when they move to `shared/sounds/`, change that one line.

The app icon and the tray icon are drawn in code, like Mochi itself:

```powershell
npm run icons          # regenerates src-tauri/icons from scripts/gen-icons.mjs
```

### Layout

```
windows/
  src/                 island front end (TypeScript, no framework)
    mochi/             Mochi and the launch greeting, in Canvas 2D
    island/            state machine, hooks, integrations
    views/             every island view
    settings/          the settings window
  src-tauri/           Rust backend: window, named pipe, Claude API, pollers
  hook/                coucou-hook.exe, the Claude Code relay
  scripts/             icon generator
```

### Log

`%LOCALAPPDATA%\Coucou\coucou.log` — hook events, permission decisions, poller
problems. It stays on your machine.

### Chat with installed CLIs

Choose Codex or Claude Code through its animated character for a personal chat;
the main view has no provider or conversation dropdown. Compatible personal
channels share their local timeline, draft, approved memories and selected
attachments. Native provider session IDs and tool permissions remain separate.

Codex uses Coucou-mediated personal tools with explicit permission. Claude can
converse with native tools disabled. Project chat for enabled providers is under
**Avançado**, defaults to read-only, and supported changes require explicit
permission.

Gemini CLI and GitHub Copilot CLI chat is unavailable in **all modes** in 0.1.2.
The Gemini 0.62.0 source review found inherited hooks/MCP configuration that can
remain active outside Coucou's approval flow. Copilot's hook/MCP isolation has
not been qualified. Choosing a project folder does not enable either chat.
Installation, sign-in and monitoring remain available for both. The ACP adapter
is retained for future qualification; see the current validation report and the
official [Gemini hooks](https://geminicli.com/docs/hooks/) and
[Copilot hooks](https://docs.github.com/en/copilot/reference/hooks-reference)
references.

CLI chat uses the installed tool's login and model settings, without monitoring
hooks. Streaming, Stop and native session resume are implemented. Resuming an
external session requires destination and full-message approval for each send.
The existing Anthropic API option keeps its separate key and history; CLI failures
never switch providers automatically.

See the [current personal-agent guide](../docs/AGENTE-PESSOAL-WINDOWS.md) and
[0.1.2 validation report](../docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md).
The [CLI chat manual](../docs/CHAT-CLI-WINDOWS.md) and
[01/10 validation report](../docs/VALIDACAO-CHAT-CLI-WINDOWS-2026-10-01.md) describe
the earlier project/session implementation and its qualification history.

## What's different from the Mac version

- No notch, so the island lives at the top centre of the screen. It remains
  compact and visible by default; hiding and pinning are user choices.
- Permission approval works from **any** terminal; the Mac build only listens to
  VS Code sessions.
- Not in this version: sending a file by email, dragging Mochi onto a window to
  attach it as context, and jumping to a specific terminal window — "Open
  terminal" opens the working folder in VS Code when `code` is on your `PATH`.
- Cal.com shows the next bookings as a list rather than the Mac's calendar.
