# NS Switch

A cross-platform desktop manager to configure and **one-click switch API providers**
across all major AI coding CLI tools — inspired by [CC Switch](https://github.com/farion1231/cc-switch).

No more hand-editing JSON / TOML config files every time you change API keys or relays.
Pick a provider in the UI (or from the **system tray**) and NS Switch writes the config
for your CLI tool atomically, with automatic backups.

Built with **Tauri 2 + Rust + Vue 3**.

## Supported tools

| Tool | Config written |
| --- | --- |
| Claude Code | `~/.claude/settings.json` (`env`) |
| Claude Desktop | `claude_desktop_config.json` (`env`) |
| Codex | `~/.codex/config.toml` (`model_provider` / `model_providers`) |
| Gemini CLI | `~/.gemini/settings.json` (`env`) |
| Grok Build | `~/.grok/settings.json` (`env`) |
| OpenCode | `~/.config/opencode/opencode.json` (`env`) |
| OpenClaw | `~/.openclaw/settings.json` (`env`) |
| Hermes Agent | `~/.hermes/settings.json` (`env`) |

## Features

### Provider management
- Add / edit / delete / duplicate providers per tool, **100+ built-in presets**
  (Official Login, DeepSeek Relay, SiliconFlow, OpenRouter, Kimi, ZetaAPI, APINebula,
  AICodeMirror, PackyCode, RunAPI, Groq, …).
- One-click `Enable` switching with atomic writes (`*.tmp` + rename).
- **Universal providers** — one config writes to Claude Code, Codex **and** Gemini CLI at once.
- **Shared config snippet** — attach extra JSON (headers, plugin config…) that is merged into
  the tool config on switch; one click to extract from your current config.
- **Drag-and-drop sorting**, import / export as JSON, import existing CLI config,
  **reset to official login** (removes ns-managed entries so OAuth works again).

### MCP · Skills · Prompts
- **Unified MCP panel** with **bidirectional sync** — servers edited directly in your CLI
  configs get imported back into the panel, and everything is synced to Claude / Claude Desktop /
  Gemini / OpenCode / Codex (`mcp_servers` in TOML).
- **Skills** — install from a GitHub repo, `.zip`, folder or **saved repository list**;
  supports **symlink or copy** install modes.
- **Prompts** — markdown presets activated to `CLAUDE.md` / `AGENTS.md` / `GEMINI.md`
  with managed markers and backfill protection.

### Usage & cost dashboard
- **Usage & cost dashboard** with per-day chart, per-model breakdown, detailed logs,
  **custom pricing** and **one-click reset / session scan** for Claude Code and Codex.

### Local proxy (hot switch)
- Each tool points at `http://127.0.0.1:<port>`, requests are forwarded to the active provider —
  **switching takes effect immediately, no CLI restart**.
- **Auto-failover** across providers, **circuit breaker** (3 failures ≈ 30 s cool-down),
  **provider health monitor** and request rectifier.
- **Per-tool takeover** — enable/disable takeover and set a **custom proxy port** per tool
  (enabled tools auto-start with the app).
- **Anthropic ↔ OpenAI protocol conversion** — point Claude Code at an OpenAI-compatible key
  (or Codex at an Anthropic key) and the proxy converts the request **and** response formats,
  including tool calls, images and **SSE streaming** (set the provider Protocol to
  `Anthropic` / `OpenAI` in the provider editor).

### Sessions · Workspace
- Browse / search Claude Code, **Codex** and **OpenCode** conversation history, open the project
  folder or restore the session (`claude --resume`) in its working directory.
- **Workspace editor** for OpenClaw agent files (`~/.openclaw/AGENTS.md`, `SOUL.md`, …)
  with Markdown preview.

### Onboarding & notifications
- **First-run onboarding** — on a fresh database NS Switch offers to import your existing
  CLI configs for every tool at once.
- **OS notifications** on provider switch and backups (toggle in Settings).

### System & platform
- **Deep Link** — `ns-switch://provider|mcp|prompt|skill?...` (also accepts `ccswitch://`):
  import via the Settings paste box, via the command line at launch
  (`ns-switch "ns-switch://provider?..."`), or OS-level after registration
  (`Register ns-switch:// scheme` in Settings; bundled builds register automatically
  through `Info.plist` on macOS and the registry on Windows).
- **Cloud sync** — move the whole data dir (Dropbox / iCloud / NAS) and **WebDAV push / pull**
  of the database across machines.
- **System tray** quick switch; dark / light / system theme; i18n **zh / zh-TW / en / ja**;
  launch-at-login; hide unused tools; one-click **backup + restore** (auto-rotated, keeps 10);
  **update check** via a configurable JSON endpoint.
- **Branded icon set** generated from an NS Switch logo (`cargo tauri icon`), no placeholder art.

## Data storage

- Database: `~/.ns-switch/ns-switch.db` (SQLite, WAL)
- Backups: `~/.ns-switch/backups/` (auto-rotated, keeps 10)
- Skills: `~/.ns-switch/skills/` (symlinked or copied into app skill dirs)
- Cloud settings: `~/.ns-switch/cloud.json` (WebDAV)

## Development

Prerequisites: Node 18+, pnpm, Rust 1.77+, Tauri 2 CLI.

```bash
# install frontend deps
pnpm install

# run dev (vite on :5200 + tauri)
pnpm tauri:dev

# backend tests (config adapters, presets, mcp sync, universal, usage)
cd src-tauri && cargo test

# typecheck + build the web frontend
pnpm build

# release bundle (.dmg / .app / etc.)
pnpm tauri:build
```

## Project layout

```
src/                 Vue 3 frontend (pages incl. Usage & Workspace, stores, i18n zh/zh-TW/en/ja)
src-tauri/src/       Rust backend
  db.rs              SQLite store + migrations + auto backup
  adapters.rs        per-tool config read/write (JSON env / Codex TOML / MCP / shared snippet)
  config.rs          write-active (incl. universal targets) + mcp sync helpers
  conv.rs            Anthropic <-> OpenAI request/response + SSE streaming conversion
  presets.rs         built-in provider presets (100+)
  tray.rs            system tray quick switch
  commands/          providers · mcp · skills · prompts · sessions(restore)
                     · settings(updates) · usage(cost) · deeplink(+scheme reg) · cloud(webdav) · workspace
src-tauri/Info.plist  macOS URL-scheme registration (ns-switch://)
src-tauri/tests/     integration tests for the core config logic
```

## License

MIT