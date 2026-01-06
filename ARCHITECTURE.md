# Architecture Documentation

Claudometer is a tray-first **macOS + Linux** desktop application that monitors Claude usage limits (not Anthropic Console billing). It is built with **Tauri v2**: a Rust backend that owns lifecycle + polling, and a small Vite-rendered settings window UI.

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
3. **Safe by default**: session keys are never logged and never persisted outside OS credential storage.

Tech choices:
- **Rust backend**: tray + polling + secure storage + updater.
- **WebView settings UI**: minimal UI, talks to Rust via `invoke` and listens for snapshot events.

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Tauri Backend (src-tauri/src)                                   │
│  • app.rs: Tauri builder + plugin wiring                         │
│  • tray.rs: tray icon + menu rendering                            │
│  • claude.rs: web + oauth usage clients + parsing/normalization   │
│  • commands.rs: invoke commands + polling loop + snapshot events  │
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

## Dual Authentication Modes

Claudometer supports two authentication modes that share the same `ClaudeUsageSnapshot` type but use different APIs:

### Web Mode (Default)

**Architecture:**
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

### CLI Mode (OAuth)

**Architecture:**
- Uses OAuth tokens from Claude Code CLI
- Fetches from `https://api.anthropic.com/api/oauth/*` endpoints
- Single account (no organization concept)
- Credentials managed by Claude CLI

**Authentication Flow:**
1. User authenticates via `claude` CLI (OAuth flow in browser)
2. Claude CLI stores tokens in `~/.claude/.credentials.json`
3. Claudometer reads credentials file (read-only, never modifies)
4. Sent as `Authorization: Bearer <token>` header

**Credential Location:**
```json
// ~/.claude/.credentials.json (managed by Claude CLI)
{
  "claudeAiOauth": {
    "accessToken": "sk-ant-...",
    "refreshToken": "sk-ant-...",
    "expiresAt": 1234567890
  }
}
```

**Key Differences:**

| Aspect | Web Mode | CLI Mode |
|--------|----------|----------|
| **Setup** | Manual cookie extraction | One-time `claude` auth |
| **Token Management** | Manual refresh needed | Auto-refresh by CLI |
| **API Endpoint** | `claude.ai/api/*` | `api.anthropic.com/api/oauth/*` |
| **Organizations** | Multi-org support | Single account |
| **Persistence** | OS credential storage (`keyring`) or memory-only | CLI manages file |
| **Credential Format** | Session cookie string | OAuth access/refresh tokens |

### Routing Logic

The Rust backend routes to the correct fetcher based on the `usageSource` setting:

```rust
if usage_source == UsageSource::Cli {
  fetch_oauth_usage_snapshot()
} else {
  fetch_claude_web_usage_snapshot(org_id)
}
```

Both services return the same `ClaudeUsageSnapshot` type, making them fully interchangeable from the UI perspective.

## Data Flow

### Startup

1. Tauri starts with **no windows**.
2. Tray menu is created immediately.
3. Polling loop triggers an initial refresh.
4. If enabled, the updater checks for updates in the background.

### Polling

On each refresh:
1. Resolve the active usage source (`web` or `cli`).
2. Web mode:
   - Resolve session key (in-memory always wins; if “Remember” is enabled, load from OS keychain/Secret Service)
   - Fetch organizations (`GET /api/organizations`) and resolve org ID
   - Fetch usage snapshot (`GET /api/organizations/:id/usage`) and normalize
3. CLI mode:
   - Read OAuth credentials from `~/.claude/.credentials.json`
   - Fetch usage snapshot (`GET https://api.anthropic.com/api/oauth/usage`) and normalize
4. Update tray menu text and emit `snapshot:updated` for settings UI.

Special behavior:
- **Unauthorized** (`401/403`) and **MissingKey**: polling pauses until the user updates settings.
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

Tray menu timestamps are handled as RFC3339 strings and formatted in `src-tauri/src/tray.rs`:
- Converted to the system local time zone via `chrono::Local` (uses OS TZ/DST rules).
- Formatted with `chrono` `unstable-locales` using a `Locale` derived from `LC_TIME` → `LC_ALL` → `LANG`.
- If parsing fails, the raw input string is displayed as a fallback.

## API Integration

Claudometer can use either Claude’s **web** endpoints (the same interface the website uses) or the Claude Code CLI OAuth usage endpoint.

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

Tracked fields (MVP):
- `five_hour` utilization
- `seven_day` utilization
- `seven_day_*` model utilization (rendered as `models[]`, preferring `seven_day_sonnet`, then `seven_day_opus`)

## Security Model

Rules:
- Never log or display the `sessionKey`.
- Never persist the `sessionKey` outside OS credential storage.
- Never log, display, or persist OAuth tokens.
- Redact any accidental `sessionKey=` occurrences from error strings.

The settings UI only accepts the session key via a password input and clears it after save.

## Debugging

For local development, set `CLAUDOMETER_DEBUG=1` to enable tray menu items that simulate near-limit and reset notifications.

## Updater + Releases

The updater uses `tauri-plugin-updater` with a static GitHub Releases manifest:

`https://github.com/<owner>/<repo>/releases/latest/download/latest.json`

Release artifacts include `.sig` signature files for the updater payloads, and `latest.json` references those signatures per platform (`linux-x86_64`, `darwin-aarch64`, `darwin-x86_64`).

See `UPDATER_SIGNING.md` for signing key setup and CI expectations.
