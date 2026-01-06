# Architecture Documentation

This document provides a detailed technical overview of the Claudometer application internals.

## Table of Contents

- [System Overview](#system-overview)
- [Component Architecture](#component-architecture)
- [Data Flow](#data-flow)
- [State Management](#state-management)
- [API Integration](#api-integration)
- [Security Model](#security-model)
- [Error Handling](#error-handling)
- [Extension Points](#extension-points)

## System Overview

Claudometer is an Electron-based tray application that monitors Claude web usage by polling the Claude.ai API. The architecture is designed around a simple polling loop that fetches usage data and updates a system tray menu.

### Design Principles

1. **Tray-first**: No main window - the app lives entirely in the system tray
2. **Minimal state**: Only essential data is persisted; most state is ephemeral
3. **Fail-safe**: Errors are surfaced in the UI but don't crash the app
4. **Security-focused**: Session keys are never logged or exposed
5. **Platform-aware**: Adapts storage strategy based on OS capabilities

### Technology Choices

| Decision | Technology | Rationale |
|----------|-----------|-----------|
| **Runtime** | Bun | Fast TypeScript execution, built-in test runner |
| **App Framework** | Electron | Cross-platform desktop with system tray support |
| **Language** | TypeScript | Type safety for complex data parsing |
| **Settings Storage** | electron-store | Simple JSON persistence for non-sensitive data |
| **Secrets Storage** | Electron safeStorage | OS-backed encryption for session key at rest |
| **HTTP Client** | Native fetch | Modern, built-in, no dependencies |
| **Linting** | Biome | Fast, batteries-included linter/formatter |

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ src/main.ts (Orchestrator)                                      │
│ • App lifecycle management                                      │
│ • Polling coordination (AppController)                          │
│ • Dependency wiring                                             │
└────┬────────────────────────────────────────────────────────────┘
     │
     ├─► AppController (src/main/app-controller.ts)
     │   • Dual routing: routes to Web or CLI service based on usageSource
     │   • refreshFromWeb() → ClaudeApiService
     │   • refreshFromCli() → ClaudeCliService → ClaudeOAuthApiService
     │
     ├─► TrayService (src/main/tray.ts)
     │   • Renders system tray icon
     │   • Builds context menu from usage snapshot
     │   • Icon color reflects status (green/red/orange/gray)
     │
     ├─► SettingsWindowService (src/main/windows/settings-window.ts)
     │   • Creates BrowserWindow
     │   • Loads Vite renderer (settings UI)
     │   • Pushes `snapshot:updated` events to renderer
     │
     ├─► SettingsService (src/main/services/settings.ts)
     │   • Wraps electron-store for type-safe access
     │   • Stores: refreshIntervalSeconds, selectedOrganizationId, rememberSessionKey, usageSource
     │
     ├─► SessionKeyService (src/main/services/session-key.ts)
     │   • In-memory session key (always) - WEB MODE ONLY
     │   • Encrypted persistence via Electron `safeStorage` (ciphertext in electron-store)
     │   • Builds "missing_key" snapshot when no key available
     │
     ├─► ClaudeApiService (src/main/services/claude-api.ts) [WEB MODE]
     │   • HTTP requests to claude.ai/api/*
     │   • fetchOrganizations() → ClaudeOrganization[]
     │   • fetchUsageSnapshot() → ClaudeUsageSnapshot
     │
     ├─► ClaudeOAuthApiService (src/main/services/claudeOAuthApi.ts) [CLI MODE]
     │   • HTTP requests to api.anthropic.com/api/oauth/*
     │   • fetchUsageSnapshot() → ClaudeUsageSnapshot
     │   • Reads OAuth token from ~/.claude/.credentials.json
     │   • No organization concept (single account)
     │
     └─► Usage Parsing (src/common/parser.ts)
         • parseClaudeUsageFromJson() → typed snapshot (Web mode)
         • OAuth API returns utilization directly (no parsing needed)
         • mapHttpStatusToUsageStatus() → error categorization
         • Smart model detection (seven_day_opus, seven_day_sonnet, etc.)
```

## Dual Authentication Modes

Claudometer supports two authentication modes that share the same `ClaudeUsageSnapshot` type but use different APIs:

### Web Mode (Default)

**Architecture:**
- Uses Claude.ai web session cookie (`sessionKey`)
- Fetches from `https://claude.ai/api/*` endpoints
- Supports multiple organizations
- Session key stored encrypted via `safeStorage`

**Authentication Flow:**
1. User extracts `sessionKey` from browser cookies
2. Session key validated by fetching organizations
3. Stored encrypted (or memory-only if encryption unavailable)
4. Sent as `Cookie: sessionKey=...` header

**Credential Location:**
- Encrypted ciphertext in `electron-store` config file
- Decrypted in-memory on app start

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
| **Persistence** | App manages encryption | CLI manages file |
| **Credential Format** | Session cookie string | OAuth access/refresh tokens |

### Routing Logic

The `AppController` routes to the correct service based on the `usageSource` setting:

```typescript
// Simplified routing logic
if (usageSource === 'cli') {
  return claudeCliService.fetchUsageSnapshot(); // → OAuth API
} else {
  // Fetch orgs, select org, then fetch usage
  return claudeApiService.fetchUsageSnapshot(orgId); // → Web API
}
```

Both services return the same `ClaudeUsageSnapshot` type, making them fully interchangeable from the UI perspective.

## Data Flow

### Startup Flow

```
1. App starts
   ↓
2. Load settings from electron-store
   ├─ usageSource: 'web' | 'cli'
   ├─ refreshIntervalSeconds
   └─ other settings
   ↓
3. Load credentials based on mode
   ├─ Web mode: Load session key from encrypted storage (if available)
   └─ CLI mode: Nothing to load (will read ~/.claude/.credentials.json on demand)
   ↓
4. Create TrayService
   ↓
5. AppController.refreshNow() / start()
   ├─ Routes to Web or CLI service based on usageSource
   └─ Fetches initial snapshot
   ↓
6. Start polling loop (single-flight setTimeout, interval from settings)
```

### Polling Loop

```
┌─────────────────────────────────────────────────────────────┐
│ AppController.refreshAll() - Every N seconds                 │
└───┬─────────────────────────────────────────────────────────┘
    │
    ├─► Check usageSource setting
    │   ├─ 'web' → refreshFromWeb()
    │   └─ 'cli' → refreshFromCli()
    │
    ├─────────────────────────┬─────────────────────────────────┐
    │                         │                                 │
    ▼ WEB MODE                ▼ CLI MODE                        │
                                                                │
    ├─► Check session key     ├─► Read ~/.claude/.credentials  │
    │   ├─ No → "missing_key" │   ├─ Missing → "unauthorized"  │
    │   └─ Yes → Continue      │   └─ Found → Continue          │
    │                         │                                 │
    ├─► Fetch organizations   ├─► (Skip - no org concept)      │
    │   ├─ Success → Cache    │                                 │
    │   └─ Error → Return     │                                 │
    │                         │                                 │
    ├─► Resolve org ID        │                                 │
    │   ├─ Use stored org     │                                 │
    │   └─ Or first org       │                                 │
    │                         │                                 │
    ├─► Fetch usage snapshot  ├─► Fetch usage snapshot         │
    │   (claude.ai/api)       │   (api.anthropic.com/oauth)    │
    │                         │                                 │
    └─────────────────────────┴─────────────────────────────────┘
                              │
                              ▼
                   ClaudeUsageSnapshot
                   (unified format)
                              │
                              ▼
    ┌─────────────────────────────────────────────────────────┐
    │ Update tray icon and menu                                │
    └─────────────────────────────────────────────────────────┘
                              │
                              ▼
    ┌─────────────────────────────────────────────────────────┐
    │ Handle special statuses                                  │
    ├─ unauthorized → Stop polling (credentials expired)       │
    └─ rate_limited → Back off (~5 minutes with jitter), retry │
    └─────────────────────────────────────────────────────────┘
```

### Settings Flow

```
User clicks "Open Settings..."
   ↓
SettingsWindow.show()
   ↓
Window loads Vite renderer UI (no Node APIs)
   ↓
User enters session key, selects options
   ↓
User clicks "Save"
   ↓
IPC call: settings:save
   ↓
Main process validates session key by fetching organizations
   ├─ Valid → Save key, update settings, start polling
   └─ Invalid → Return error, show in UI
```

## State Management

### Persistent State (electron-store)

Stored in `~/.config/claudometer/config.json` (Linux) or `~/Library/Application Support/claudometer/config.json` (macOS):

```typescript
{
  refreshIntervalSeconds: number,        // Default: 60
  selectedOrganizationId?: string,       // UUID of selected org
  rememberSessionKey: boolean            // Default: false
}
```

### Ephemeral State (In-Memory)

```typescript
// src/main/app-controller.ts (conceptually)
// - organizations: ClaudeOrganization[] (cached after validation / org fetch)
// - latestSnapshot: ClaudeUsageSnapshot | null (drives tray + settings UI)
// - timer: NodeJS.Timeout | null (single-flight setTimeout scheduler)
// - currentRun: Promise<void> | null (prevents overlapping polls)
// - pendingImmediate: boolean (coalesces refreshNow calls)
```

### Secure State (Encrypted Storage)

When `safeStorage.isEncryptionAvailable()` is true, the session key is stored only as encrypted ciphertext (base64) in `electron-store`, and decrypted only in the main process.

## API Integration

### Overview

Claudometer integrates with **two different APIs** depending on the authentication mode:

1. **Claude Web API** (`claude.ai/api/*`) - Used in Web mode
2. **Anthropic OAuth API** (`api.anthropic.com/api/oauth/*`) - Used in CLI mode

Both APIs return compatible usage data that maps to the same `ClaudeUsageSnapshot` type.

### Claude Web API (Web Mode)

The app integrates with Claude's web API (not the official Anthropic API). These are the same endpoints used by the claude.ai web interface.

#### Authentication

```http
GET /api/organizations
Cookie: sessionKey=sk-ant-sid01-...
```

The `sessionKey` is extracted from browser cookies and sent in the Cookie header.

#### Endpoints

##### GET /api/organizations

**Purpose**: List available organizations for the authenticated user

**Request**:
```http
GET https://claude.ai/api/organizations
Cookie: sessionKey=sk-ant-sid01-...
User-Agent: Mozilla/5.0 ...
Origin: https://claude.ai
Referer: https://claude.ai/
```

**Response** (200 OK):
```json
[
  {
    "uuid": "org-123abc...",
    "name": "Personal",
    "created_at": "2024-01-15T10:00:00.000Z",
    ...
  }
]
```

**Parsed to**:
```typescript
type ClaudeOrganization = {
  id: string;        // from uuid
  name?: string;     // from name
}
```

##### GET /api/organizations/:id/usage

**Purpose**: Fetch current usage statistics for an organization

**Request**:
```http
GET https://claude.ai/api/organizations/org-123abc.../usage
Cookie: sessionKey=sk-ant-sid01-...
```

**Response** (200 OK):
```json
{
  "five_hour": {
    "utilization": 23.45,
    "resets_at": "2024-01-15T15:00:00.000Z"
  },
  "seven_day": {
    "utilization": 67.89,
    "resets_at": "2024-01-22T00:00:00.000Z"
  },
  "seven_day_opus": {
    "utilization": 12.34,
    "resets_at": "2024-01-22T00:00:00.000Z"
  }
}
```

**Notes**:
- `utilization` is a percentage (0-100)
- Some accounts have `seven_day_sonnet` instead of `seven_day_opus`
- The parser detects all `seven_day_*` keys dynamically

**Parsed to**:
```typescript
type ClaudeUsageSnapshot = {
  status: 'ok';
  organizationId: string;
  sessionPercent: number;        // five_hour.utilization
  sessionResetsAt?: string;      // five_hour.resets_at
  weeklyPercent: number;         // seven_day.utilization
  weeklyResetsAt?: string;       // seven_day.resets_at
  modelWeeklyPercent: number;    // seven_day_*.utilization
  modelWeeklyName?: string;      // 'Opus' | 'Sonnet' | ...
  modelWeeklyResetsAt?: string;
  lastUpdatedAt: string;         // ISO 8601 timestamp
}
```

### Anthropic OAuth API (CLI Mode)

The OAuth API provides usage data directly from Anthropic, accessed via OAuth tokens managed by Claude Code CLI.

#### Authentication

```http
GET /api/oauth/usage
Authorization: Bearer <access_token>
anthropic-beta: oauth-2025-04-20
```

The `access_token` is read from `~/.claude/.credentials.json` (managed by Claude CLI).

#### Credentials File Structure

**Location**: `~/.claude/.credentials.json`

**Format**:
```json
{
  "claudeAiOauth": {
    "accessToken": "sk-ant-...",
    "refreshToken": "sk-ant-...",
    "expiresAt": 1234567890
  }
}
```

**Management**:
- Created by `claude` CLI during OAuth flow
- Token refresh handled automatically by CLI
- Claudometer only reads (never writes or modifies)
- Multiple apps can share the same credentials file

#### Endpoint

##### GET /api/oauth/usage

**Purpose**: Fetch current usage statistics for the authenticated account

**Request**:
```http
GET https://api.anthropic.com/api/oauth/usage
Authorization: Bearer sk-ant-...
anthropic-beta: oauth-2025-04-20
```

**Response** (200 OK):
```json
{
  "five_hour": {
    "utilization": 9,
    "resets_at": "2026-01-06T18:00:00.000Z"
  },
  "seven_day": {
    "utilization": 38,
    "resets_at": "2026-01-13T00:00:00.000Z"
  },
  "seven_day_opus": null,
  "seven_day_sonnet": {
    "utilization": 26,
    "resets_at": "2026-01-13T00:00:00.000Z"
  }
}
```

**Key Differences from Web API**:
- `utilization` is already a percentage (no units_used/units_limit)
- Model metrics can be `null` (need fallback logic)
- No organization concept (single account)
- Simpler response structure

**Parsed to**: Same `ClaudeUsageSnapshot` type as Web mode
```typescript
{
  status: 'ok',
  organizationId: 'oauth',  // Static identifier (no real org)
  sessionPercent: 9,
  weeklyPercent: 38,
  modelWeeklyPercent: 26,
  modelWeeklyName: 'Sonnet',
  // ... other fields
}
```

**Model Fallback Logic**:
```typescript
// Prefer Opus, fallback to Sonnet, then 0
if (data.seven_day_opus) {
  modelWeeklyPercent = data.seven_day_opus.utilization;
  modelWeeklyName = 'Opus';
} else if (data.seven_day_sonnet) {
  modelWeeklyPercent = data.seven_day_sonnet.utilization;
  modelWeeklyName = 'Sonnet';
} else {
  modelWeeklyPercent = 0;
  modelWeeklyName = undefined;
}
```

### Error Handling in API Client

HTTP status codes are mapped to usage statuses:

| HTTP Status | Usage Status | Behavior |
|------------|--------------|----------|
| 401, 403 | `unauthorized` | Stop polling (session key expired) |
| 429 | `rate_limited` | Stop polling, retry in 5 minutes |
| Other 4xx/5xx | `error` | Continue polling, show error message |

Error snapshots include the error message:
```typescript
{
  status: 'error' | 'unauthorized' | 'rate_limited',
  organizationId?: string,
  lastUpdatedAt: string,
  errorMessage?: string
}
```

## Security Model

### Credential Protection

**Design goal**: Never expose authentication credentials outside their designated secure storage.

#### Web Mode - Session Key Protection

**Storage Strategy:**

| Platform | Enabled | Storage Location | Persistence |
|----------|---------|------------------|-------------|
| macOS | Usually | `safeStorage` + `electron-store` ciphertext | Across app restarts |
| Linux | Depends on DE/keyring | `safeStorage` + `electron-store` ciphertext | Across app restarts (if encryption available) |
| Any | If encryption unavailable | In-memory only | Until app exits |

**Security Measures:**

1. **No logging**: Session key never appears in logs (sanitized via `sanitizeErrorMessage`)
2. **No error messages**: Session key never included in error text shown to user
3. **Redacted debug logs**: When `CLAUDE_USAGE_DEBUG=1`, session keys are redacted as `REDACTED`
4. **Validation before storage**: Keys are tested against Claude API before saving
5. **Password input**: UI uses `<input type="password">` to prevent shoulder-surfing

#### CLI Mode - OAuth Token Protection

**Storage Strategy:**

- Tokens stored in `~/.claude/.credentials.json` by Claude CLI
- Claudometer only **reads** the file (never writes or modifies)
- File permissions: `600` (user read/write only)
- Managed entirely by Claude CLI (auto-refresh)

**Security Measures:**

1. **Read-only access**: Claudometer never modifies OAuth credentials
2. **No caching**: Tokens read fresh on every request
3. **No logging**: OAuth tokens never logged (same redaction as session keys)
4. **File permissions**: CLI creates file with restrictive permissions
5. **Token refresh**: Handled automatically by Claude CLI (not Claudometer)

#### Code Example: Redaction

```typescript
function sanitizeErrorMessage(message: string): string {
  return message.replaceAll(/sessionKey=[^;\s]+/gi, 'sessionKey=REDACTED');
}

function redactBodyForLogs(body: string): string {
  let redacted = body.replaceAll(/sessionKey=[^;\s]+/gi, 'sessionKey=REDACTED');
  redacted = redacted.replaceAll(/sk-ant-sid01-[A-Za-z0-9_-]+/g, 'sk-ant-sid01-REDACTED');
  return redacted;
}
```

### IPC Security

The settings window uses `nodeIntegration: false` and `contextIsolation: true`, and exposes a minimal API via the preload script (`src/preload/preload.ts`) as `window.api`.

**IPC handlers** (`src/main/ipc/register.ts`):
```typescript
import { ipcMain } from 'electron';
import { ipcChannels } from '../../common/ipc.ts';

ipcMain.handle(ipcChannels.settings.getState, async () => controller.getState());
ipcMain.handle(ipcChannels.settings.save, async (_event, payload) => controller.saveSettings(payload));
ipcMain.handle(ipcChannels.settings.forgetKey, async () => controller.forgetKey());
ipcMain.handle(ipcChannels.settings.refreshNow, async () => controller.refreshNow());
```

## Error Handling

### Error Categories

| Category | Status | User Impact | Recovery |
|----------|--------|-------------|----------|
| Missing key | `missing_key` | Tray shows "needs session key" | User opens settings, adds key |
| Unauthorized | `unauthorized` | Tray red, polling stops | User refreshes session key |
| Rate limited | `rate_limited` | Tray orange, backs off polling | Auto-retry in ~5 minutes |
| Network/API error | `error` | Error message in tray | Continues polling (transient) |

### Graceful Degradation

- **No session key**: App runs but shows "missing_key" status
- **Invalid session key**: Validation fails in settings UI, existing state unchanged
- **API errors**: Tray shows last-known good data until next successful fetch
- **Encryption unavailable**: Session key cannot persist; UI warns and app runs memory-only

### Polling State Machine

```
                 ┌─────────────┐
                 │   STOPPED   │
                 └──────┬──────┘
                        │
          ┌─────────────▼──────────────┐
          │      POLLING (active)      │
          │  Timer running, fetching   │
          └─┬──────────────────────┬───┘
            │                      │
            │ unauthorized         │ rate_limited
            │ (session expired)    │ (429 error)
            │                      │
            ▼                      ▼
    ┌─────────────┐        ┌──────────────┐
    │   STOPPED   │        │   BACKOFF    │
    │ (needs key) │        │ (~5min)      │
    └─────────────┘        └──────┬───────┘
                                  │
                            backoff elapsed
                                  │
                                  ▼
                           Next poll attempt
```

## Extension Points

### Adding New Usage Metrics

To track additional metrics from the Claude API response:

1. **Update types** in `src/common/types.ts`:
   ```typescript
   export type ClaudeUsageSnapshot = {
     status: 'ok';
     // ... existing fields
     newMetricPercent: number;        // Add new field
     newMetricResetsAt?: string;
   }
   ```

2. **Update parser** in `src/common/parser.ts`:
   ```typescript
   export function parseClaudeUsageFromJson(...): ClaudeUsageSnapshot {
     const root = readObject(json) ?? {};
     const newMetric = readObject(root.new_metric);

     return {
       // ... existing fields
       newMetricPercent: parseUtilizationPercent(newMetric?.utilization),
       newMetricResetsAt: readString(newMetric?.resets_at),
     };
   }
   ```

3. **Update tray display** in `src/main/tray.ts`:
   ```typescript
   items.push({
     label: `New Metric: ${this.formatPercent(snapshot.newMetricPercent)}`,
     enabled: false,
   });
   ```

### Adding New Settings

To add a new persistent setting:

1. **Update SettingsService** in `src/main/services/settings.ts`:
   ```typescript
   getMyNewSetting(): string {
     return this.store.get('myNewSetting', 'default');
   }

   setMyNewSetting(value: string): void {
     this.store.set('myNewSetting', value);
   }
   ```

2. **Add to settings UI** in `src/renderer/settings/main.ts` (renderer)
3. **Wire up in save handler** (renderer → preload → ipcMain):
   ```javascript
   const payload = {
     // ... existing fields
     myNewSetting: el('myNewSetting').value,
   };
   ```

### Supporting Windows

Current blockers for Windows support:
2. **Tray icon rendering**: Uses raw RGBA buffer; test compatibility
3. **Testing**: No Windows CI/testing currently

To add Windows support:
1. Validate `safeStorage` behavior on Windows (encryption availability and persistence)
2. Test tray icon rendering on Windows
3. Update `package.json` platform targets
4. Add Windows-specific build configuration

### Adding Desktop Notifications

To notify users when approaching usage limits:

```typescript
// src/main.ts
import { Notification } from 'electron';

let notifiedAboutSession = false;

controller.onSnapshotUpdated((snapshot) => {
  if (snapshot?.status !== 'ok') return;
  if (snapshot.sessionPercent <= 90) return;
  if (notifiedAboutSession) return;

  new Notification({
    title: 'Claude Usage Warning',
    body: `Session usage at ${Math.round(snapshot.sessionPercent)}%`,
  }).show();
  notifiedAboutSession = true;
});
```

Add a setting to control notification thresholds.

## Testing Strategy

### Current Coverage

- **Unit tests**: `usageParser.test.ts` covers JSON parsing edge cases
- **Manual testing**: Run app and verify tray behavior

### Recommended Testing

| Test Type | Scope | Tools |
|-----------|-------|-------|
| **Unit** | Pure functions (parsers, formatters) | Bun test |
| **Integration** | API client with mocked fetch | Bun test + MSW |
| **E2E** | Full app with mocked Claude API | Playwright for Electron |

### Example: Testing Parser

```typescript
// src/common/usageParser.test.ts
import { describe, expect, test } from 'bun:test';
import { parseUtilizationPercent } from './usageParser.ts';

describe('parseUtilizationPercent', () => {
  test('clamps values above 100', () => {
    expect(parseUtilizationPercent(150)).toBe(100);
  });

  test('handles string input', () => {
    expect(parseUtilizationPercent('75.5')).toBe(75.5);
  });
});
```

## Performance Considerations

### Polling Frequency

- **Minimum interval**: 10 seconds (enforced in settings)
- **Default**: 60 seconds (good balance of freshness vs API load)
- **Recommendation**: 60-300 seconds for normal use

Too-frequent polling may trigger rate limiting (429 errors).

### Memory Usage

- Minimal: ~50-80 MB (typical Electron overhead)
- No memory leaks observed (tray menu rebuilt on each update, old menu GC'd)

### Startup Time

- Typical: <1 second to tray icon visible
- First API call: 1-2 seconds (fetches organizations, then usage)

## Debugging

### Enable Debug Logging

```bash
CLAUDE_USAGE_DEBUG=1 bun run dev
```

Logs all API requests/responses (with session keys redacted).

### Inspect Stored Settings

**macOS**:
```bash
cat ~/Library/Application\ Support/claudometer/config.json
```

**Linux**:
```bash
cat ~/.config/claudometer/config.json
```

### Check Keytar Storage (macOS)

```bash
security find-generic-password -s claudometer -w
```

## Known Limitations

1. **Claude Web API instability**: The web API is not officially documented and may change without notice
2. **No official API support**: This app uses the web interface's private API, not the Anthropic API
3. **Linux session persistence**: No secure storage on Linux; session key lost on app restart
4. **Single instance**: Only one instance of the app can run at a time
5. **No historical data**: Usage stats are ephemeral; no database or historical tracking
