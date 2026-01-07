<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# Claudometer (Tauri)

This file mirrors `AGENTS.md` for tooling that auto-loads `CLAUDE.md`.

## Project Overview

**Claudometer** is a tray-first desktop application for **macOS + Linux** that shows Claude usage limits in near real time.

The MVP tracks Claude usage via **two authentication modes**:
- **Web mode**: Claude.ai web session cookie (`sessionKey`)
- **CLI mode**: Claude Code CLI OAuth credentials (local Claude Code session)

Both modes track the same metrics:
- 5-hour session utilization (`five_hour`)
- weekly utilization (`seven_day`)
- model-specific weekly utilization (`seven_day_*`, prefers `seven_day_sonnet`, then `seven_day_opus`)

**Web mode** authenticates via Claude web session cookie, sent as `Cookie: sessionKey=...` to `https://claude.ai/api/*`.

**CLI mode** authenticates via OAuth Bearer token, sent to `https://api.anthropic.com/api/oauth/*`.

## Tech Stack (Target)

| Layer | Technologies |
|-------|--------------|
| App | Tauri v2 (Rust backend) + TypeScript UI |
| Runtime | Bun |
| Settings | `tauri-plugin-store` (non-sensitive) |
| Secrets | OS Keychain/Secret Service via `keyring` (session key) |
| Formatting/Lint | Biome |
| Tests | Lightweight unit tests for parsing (framework TBD; prefer minimal) |

## Repository Structure (Target)

```
claudometer/
├── src-tauri/             # Tauri backend (tray, polling, commands, bundling)
├── src/
│   ├── renderer/settings/ # Settings window UI (Tauri invoke + events)
│   └── common/            # Shared UI types
├── assets/                # Tray icons
├── openspec/              # Specs and change proposals
├── package.json
├── tsconfig.json
├── AGENTS.md
└── CLAUDE.md              # This file
```

## Development Workflow

This repo uses OpenSpec for planning/requirements:
- Create proposal: `openspec list`, then add change under `openspec/changes/<change-id>/`, then `openspec validate <id> --strict`
- Implement after approval: follow `openspec/changes/<id>/tasks.md`

## Security & Privacy Rules

- Never log or persist the Claude `sessionKey` outside OS credential storage.
- Never include the session key in error messages, UI text, or telemetry.
- Assume Claude web endpoints can change; handle errors and unauthorized states gracefully.
