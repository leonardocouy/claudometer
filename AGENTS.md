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

# Claudometer (Electron)

## Project Overview

**Claudometer** is a tray-first desktop application for **macOS + Linux** that shows Claude web usage limits in near real time.

The MVP tracks **Claude web** usage (not Anthropic Console billing):
- 5-hour session utilization (`five_hour`)
- weekly utilization (`seven_day`)
- weekly Opus utilization (`seven_day_opus`)

Authentication is via the Claude web session cookie (`sessionKey`), sent as `Cookie: sessionKey=...` to `https://claude.ai/api/...`.

## Tech Stack (Target)

| Layer | Technologies |
|-------|--------------|
| App | Electron + TypeScript |
| Runtime | Bun |
| Settings | `electron-store` (non-sensitive) |
| Secrets | OS credential store via `keytar` (session key) |
| Formatting/Lint | Biome |
| Tests | Lightweight unit tests for parsing (framework TBD; prefer minimal) |

## Repository Structure (Target)

```
claudometer/
├── src/
│   ├── main.ts            # Electron main process entry (tray-first)
│   ├── main/              # Main process modules (tray, settings window, polling, IPC)
│   ├── preload/           # Secure bridge (contextIsolation) exposing window.api
│   ├── renderer/          # Vite renderers (settings window UI)
│   └── common/            # Shared types + parsing + IPC contract
├── assets/                # Tray icons
├── openspec/              # Specs and change proposals
├── package.json
├── tsconfig.json
├── AGENTS.md              # This file
└── CLAUDE.md              # Mirror of AGENTS.md (for tooling)
```

## Development Workflow

This repo uses OpenSpec for planning/requirements:
- Create proposal: `openspec list`, then add change under `openspec/changes/<change-id>/`, then `openspec validate <id> --strict`
- Implement after approval: follow `openspec/changes/<id>/tasks.md`

## Security & Privacy Rules

- Never log or persist the Claude `sessionKey` outside OS credential storage.
- Never include the session key in error messages, UI text, or telemetry.
- Assume Claude web endpoints can change; handle errors and unauthorized states gracefully.
