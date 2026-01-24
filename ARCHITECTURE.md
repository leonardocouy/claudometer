# Architecture Documentation

Claudometer is a tray-first **macOS + Linux** desktop application that monitors **Claude** and **Codex** usage limits (not Anthropic Console billing). It is built with **Tauri v2**: a Rust backend that owns lifecycle + polling, and a small Vite-rendered settings window UI.

## Table of Contents

- [System Overview](#system-overview)
- [Component Architecture](#component-architecture)
- [Data Flow](#data-flow)
- [State Management](#state-management)
- [API Integration](#api-integration)
- [Security Model](#security-model)
- [Updater + Releases](#updater--releases)

## System Overview

Design principles:
1. **Tray-first**: no window at startup; settings window is opened on demand.
2. **Single polling loop**: a single-flight refresh loop with backoff.
3. **Safe by default**: secrets/tokens are never logged and never persisted outside OS credential storage.

Tech choices:
- **Rust backend**: tray + polling + secure storage + updater.
- **WebView settings UI**: minimal UI, talks to Rust via `invoke` and listens for snapshot events.

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Tauri Backend (src-tauri/src)                                   │
│  • app.rs: Tauri builder + plugin wiring                         │
│  • tray/: tray icon + menu rendering                              │
│  • refresh/: refresh loop + backoff + fetch bundle                │
│  • state/: shared state (settings, caches, secrets)               │
│  • claude.rs: Claude web + oauth usage clients + parsing           │
│  • codex.rs: Codex oauth + CLI usage clients + parsing             │
│  • commands/: invoke commands (settings, updates)                  │
│  • notifications.rs: wiring for near-limit/reset notifications      │
│  • settings.rs: tauri-plugin-store persistence (non-sensitive)    │
│  • usage_alerts.rs: near-limit + reset notification decisions     │
│  • updater.rs: tauri-plugin-updater integration                   │
│  • windows.rs: create/focus settings window on demand             │
└───────────────┬──────────────────────────────────────────────────┘
                │ invoke / events
                ▼
┌─────────────────────────────────────────────────────────────────┐
│ Settings UI (src/renderer/settings)                              │
│  • Uses @tauri-apps/api invoke commands                           │
│  • Subscribes to `snapshot:updated` events                        │
│  • Uses plugin-opener to open GitHub links                         │
└─────────────────────────────────────────────────────────────────┘
```

## Providers and Authentication

Claudometer can track either (or both) providers:
- **Claude (Anthropic)** usage (session + weekly + model weekly)
- **Codex (OpenAI)** usage (session + weekly)

Snapshots are combined into a single `UsageSnapshotBundle` and emitted to the UI via the `snapshot:updated` event.

### Claude: Web Mode

- Uses Claude.ai web session cookie (`sessionKey`)
- Fetches from `https://claude.ai/api/*` endpoints
- Supports multiple organizations
- Session key is stored only in OS credential storage via Rust `keyring` (optional)

**Authentication Flow:**
1. User extracts `sessionKey` from browser cookies
2. Session key validated by fetching organizations
3. Stored encrypted (or memory-only if encryption unavailable)
4. Sent as `Cookie: sessionKey=...` header

**Credential Location:**
- If “Remember session key” is enabled: stored in OS credential storage via Rust `keyring`
- Otherwise: kept in-memory only for the current app session

### Claude: CLI Mode (OAuth)

**Architecture:**
- Uses OAuth tokens from Claude Code
- Fetches from `https://api.anthropic.com/api/oauth/*` endpoints
- Single account (no organization concept)
- Credentials managed by Claude CLI

**Authentication Flow:**
1. User authenticates via `claude` CLI (OAuth flow in browser)
2. Claude CLI stores tokens locally after login
3. Claudometer reads those credentials (read-only, never modifies)
4. Sent as `Authorization: Bearer <token>` header

**Credential Location:** Managed locally by Claude Code (not stored by Claudometer).

### Codex: OAuth Mode

- Reads local Codex credentials (e.g. `~/.codex/auth.json` or `$CODEX_HOME/auth.json`).
- Calls Codex usage endpoints over HTTPS with `Authorization: Bearer ...`.
- May include `chatgpt-account-id` when present in local credentials.

### Codex: CLI Mode

- Shells out to the local `codex` binary and parses structured output.
- No credentials are stored by Claudometer in this mode.

## Data Flow

### Startup

1. Tauri starts with **no windows**.
2. Tray menu is created immediately.
3. Polling loop triggers an initial refresh.
4. If enabled, the updater checks for updates in the background.

### Polling

On each refresh:
1. Read current settings (enabled providers + sources).
2. Claude web mode (when enabled):
   - Resolve session key (in-memory always wins; if “Remember” is enabled, load from OS keychain/Secret Service)
   - Fetch organizations (`GET /api/organizations`) and resolve org ID
   - Fetch usage snapshot (`GET /api/organizations/:id/usage`) and normalize
3. Claude CLI mode (when enabled):
   - Read OAuth credentials from the local Claude Code session
   - Fetch usage snapshot (`GET https://api.anthropic.com/api/oauth/usage`) and normalize
4. Codex (when enabled):
   - OAuth mode: read local auth + fetch usage snapshot over HTTPS
   - CLI mode: execute `codex` and parse usage
5. Bundle snapshots and update tray menu, then emit `snapshot:updated` for settings UI.

Special behavior:
- **Unauthorized** (`401/403`) and **MissingKey**: polling pauses only when *all enabled* providers are blocked (so the other provider can continue updating).
- **Rate limited** (`429`): polling backs off with jitter.

## State Management

### Non-sensitive persistence

Stored via `tauri-plugin-store`:
- refresh interval
- selected org ID
- remember flag
- usage source (`web` | `cli`)
- autostart preference
- updater preferences
- notification markers (near-limit + reset dedupe)

### Sensitive persistence

The Claude `sessionKey` is stored only using OS credential storage (`keyring` crate):
- **macOS**: Keychain
- **Linux**: Secret Service

If OS credential storage is unavailable, “Remember session key” is disabled (no file fallback).

## Date/Time Handling

Tray menu timestamps are handled as RFC3339 strings and formatted in `src-tauri/src/tray/formatters.rs`:
- Converted to the system local time zone via `chrono::Local` (uses OS TZ/DST rules).
- Formatted with `chrono` `unstable-locales` using a `Locale` derived from `LC_TIME` → `LC_ALL` → `LANG`.
- If parsing fails, the raw input string is displayed as a fallback.

## API Integration

Claudometer can use Claude’s **web** endpoints (the same interface the website uses), the Claude Code OAuth usage endpoint, and Codex usage endpoints.

Authentication:

```http
Cookie: sessionKey=...
```

CLI/OAuth authentication:
```http
Authorization: Bearer <token>
anthropic-beta: oauth-2025-04-20
```

Endpoints:
- `GET https://claude.ai/api/organizations`
- `GET https://claude.ai/api/organizations/:id/usage`
- `GET https://api.anthropic.com/api/oauth/usage`
- `GET https://chatgpt.com/backend-api/wham/usage` (Codex, primary)
- `GET https://chatgpt.com/api/codex/usage` (Codex, fallback)

Tracked fields (MVP):
- `five_hour` utilization
- `seven_day` utilization
- `seven_day_*` model utilization (rendered as `models[]`, preferring `seven_day_sonnet`, then `seven_day_opus`)

## Security Model

Rules:
- Never log or display the `sessionKey`.
- Never persist the `sessionKey` outside OS credential storage.
- Never log, display, or persist OAuth tokens (Claude or Codex).
- Redact any accidental `sessionKey=` occurrences from error strings.

The settings UI only accepts the session key via a password input and clears it after save.

## Debugging

For local development, set `CLAUDOMETER_DEBUG=1` to enable tray menu items that simulate near-limit and reset notifications.

## Updater + Releases

The updater uses `tauri-plugin-updater` with a static GitHub Releases manifest:

`https://github.com/<owner>/<repo>/releases/latest/download/latest.json`

Release artifacts include `.sig` signature files for the updater payloads, and `latest.json` references those signatures per platform (`linux-x86_64`, `darwin-aarch64`, `darwin-x86_64`).

See `UPDATER_SIGNING.md` for signing key setup and CI expectations.
