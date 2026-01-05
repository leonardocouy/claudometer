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
| **Secrets Storage** | keytar (macOS) | OS-level credential storage for session keys |
| **HTTP Client** | Native fetch | Modern, built-in, no dependencies |
| **Linting** | Biome | Fast, batteries-included linter/formatter |

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ main/index.ts (Orchestrator)                                    │
│ • App lifecycle management                                      │
│ • Polling coordination                                          │
│ • Dependency wiring                                             │
└────┬────────────────────────────────────────────────────────────┘
     │
     ├─► TrayManager (main/tray.ts)
     │   • Renders system tray icon
     │   • Builds context menu from usage snapshot
     │   • Icon color reflects status (green/red/orange/gray)
     │
     ├─► SettingsWindow (main/settingsWindow.ts)
     │   • Inline HTML UI (no separate file)
     │   • IPC communication with main process
     │   • Session key input, org selection, refresh interval
     │
     ├─► SettingsManager (main/settings.ts)
     │   • Wraps electron-store for type-safe access
     │   • Stores: refreshIntervalSeconds, selectedOrganizationId, rememberSessionKey
     │
     ├─► SessionKeyStore (main/sessionKeyStore.ts)
     │   • In-memory session key (always)
     │   • Keytar storage (macOS only, if "Remember" enabled)
     │   • Builds "missing_key" snapshot when no key available
     │
     ├─► ClaudeWebUsageClient (main/claudeWebUsageClient.ts)
     │   • HTTP requests to claude.ai/api/*
     │   • fetchOrganizations() → ClaudeOrganization[]
     │   • fetchUsageSnapshot() → ClaudeUsageSnapshot
     │
     └─► Usage Parsing (shared/usageParser.ts)
         • parseClaudeUsageFromJson() → typed snapshot
         • mapHttpStatusToUsageStatus() → error categorization
         • Smart model detection (seven_day_opus, seven_day_sonnet, etc.)
```

## Data Flow

### Startup Flow

```
1. App starts
   ↓
2. Load settings from electron-store
   ↓
3. Load session key from keytar (macOS) or in-memory
   ↓
4. Create TrayManager
   ↓
5. Call refreshAll()
   ↓
6. Start polling timer (interval from settings)
```

### Polling Loop

```
┌─────────────────────────────────────────────────────────────┐
│ refreshAll() - Every N seconds                              │
└───┬─────────────────────────────────────────────────────────┘
    │
    ├─► Check if session key exists
    │   ├─ No → Show "missing_key" status, stop polling
    │   └─ Yes → Continue
    │
    ├─► Fetch organizations (if not cached or org changed)
    │   ├─ Success → Cache organizations
    │   └─ Error → Show error in tray, return
    │
    ├─► Resolve organization ID
    │   ├─ Use stored org if valid
    │   ├─ Otherwise use first org
    │   └─ Store resolved org ID
    │
    ├─► Fetch usage snapshot
    │   └─ Returns ClaudeUsageSnapshot (status: ok | error | unauthorized | rate_limited)
    │
    ├─► Update tray icon and menu
    │
    └─► Handle special statuses
        ├─ unauthorized → Stop polling (key expired)
        └─ rate_limited → Stop polling, retry in 5 minutes
```

### Settings Flow

```
User clicks "Open Settings..."
   ↓
SettingsWindow.show()
   ↓
Window loads inline HTML with IPC handlers
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
// main/index.ts
let organizations: ClaudeOrganization[] = [];    // Fetched on demand
let latestSnapshot: ClaudeUsageSnapshot | null = null;  // Updated every poll
let pollTimer: NodeJS.Timeout | null = null;     // Active polling interval
```

### Secure State (OS Keychain)

**macOS**: Stored in system Keychain via `keytar`
- Service: `claudometer`
- Account: `session-key`
- Value: The Claude sessionKey cookie

**Linux**: In-memory only (not persisted to disk)

## API Integration

### Claude Web API

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

### Session Key Protection

**Design goal**: Never expose the Claude session key outside secure storage.

#### Storage Strategy

| Platform | Enabled | Storage Location | Persistence |
|----------|---------|------------------|-------------|
| macOS | Always | System Keychain (via keytar) | Across app restarts |
| Linux | Optional | Memory only | Lost on app quit |

#### Security Measures

1. **No logging**: Session key never appears in logs (sanitized via `sanitizeErrorMessage`)
2. **No error messages**: Session key never included in error text shown to user
3. **Redacted debug logs**: When `CLAUDE_USAGE_DEBUG=1`, session keys are redacted as `REDACTED`
4. **Validation before storage**: Keys are tested against Claude API before saving
5. **Password input**: UI uses `<input type="password">` to prevent shoulder-surfing

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

The settings window uses `nodeIntegration: true` and `contextIsolation: false` to simplify IPC, since:
- The HTML is inline (no risk of loading untrusted remote content)
- There's no user-generated content rendered in the window
- The app doesn't navigate to external URLs

**IPC handlers** (main/settingsWindow.ts):
```typescript
ipcMain.handle('settings:getState', async () => this.getState());
ipcMain.handle('settings:save', async (_event, payload) => this.onSave(payload));
ipcMain.handle('settings:forgetKey', async () => this.onForgetKey());
ipcMain.handle('settings:refreshNow', async () => this.onRefreshNow());
```

All handlers are cleaned up when the settings window closes.

## Error Handling

### Error Categories

| Category | Status | User Impact | Recovery |
|----------|--------|-------------|----------|
| Missing key | `missing_key` | Tray shows "needs session key" | User opens settings, adds key |
| Unauthorized | `unauthorized` | Tray red, polling stops | User refreshes session key |
| Rate limited | `rate_limited` | Tray orange, polling paused | Auto-retry in 5 minutes |
| Network/API error | `error` | Error message in tray | Continues polling (transient) |

### Graceful Degradation

- **No session key**: App runs but shows "missing_key" status
- **Invalid session key**: Validation fails in settings UI, existing state unchanged
- **API errors**: Tray shows last-known good data until next successful fetch
- **Keytar unavailable (Linux)**: Falls back to in-memory storage

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
    │   STOPPED   │        │   PAUSED     │
    │ (needs key) │        │ (retry 5min) │
    └─────────────┘        └──────┬───────┘
                                  │
                          5 minutes elapsed
                                  │
                                  ▼
                          Resume polling
```

## Extension Points

### Adding New Usage Metrics

To track additional metrics from the Claude API response:

1. **Update types** in `shared/claudeUsage.ts`:
   ```typescript
   export type ClaudeUsageSnapshot = {
     status: 'ok';
     // ... existing fields
     newMetricPercent: number;        // Add new field
     newMetricResetsAt?: string;
   }
   ```

2. **Update parser** in `shared/usageParser.ts`:
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

3. **Update tray display** in `main/tray.ts`:
   ```typescript
   items.push({
     label: `New Metric: ${this.formatPercent(snapshot.newMetricPercent)}`,
     enabled: false,
   });
   ```

### Adding New Settings

To add a new persistent setting:

1. **Update SettingsManager** in `main/settings.ts`:
   ```typescript
   getMyNewSetting(): string {
     return this.store.get('myNewSetting', 'default');
   }

   setMyNewSetting(value: string): void {
     this.store.set('myNewSetting', value);
   }
   ```

2. **Add to settings UI** in `main/settingsWindow.ts` (inline HTML):
   ```html
   <div class="row">
     <label for="myNewSetting">My New Setting</label>
     <input id="myNewSetting" type="text" />
   </div>
   ```

3. **Wire up in save handler**:
   ```javascript
   const payload = {
     // ... existing fields
     myNewSetting: el('myNewSetting').value,
   };
   ```

### Supporting Windows

Current blockers for Windows support:
1. **Keytar**: Optional dependency, may need alternative (e.g., node-credential-manager)
2. **Tray icon rendering**: Uses raw RGBA buffer; test compatibility
3. **Testing**: No Windows CI/testing currently

To add Windows support:
1. Test keytar or implement alternative credential storage
2. Test tray icon rendering on Windows
3. Update `package.json` platform targets
4. Add Windows-specific build configuration

### Adding Desktop Notifications

To notify users when approaching usage limits:

```typescript
// main/index.ts
import { Notification } from 'electron';

function updateTray(snapshot: ClaudeUsageSnapshot | null): void {
  latestSnapshot = snapshot;

  if (snapshot?.status === 'ok') {
    // Check threshold (e.g., 90%)
    if (snapshot.sessionPercent > 90 && !notifiedAboutSession) {
      new Notification({
        title: 'Claude Usage Warning',
        body: `Session usage at ${Math.round(snapshot.sessionPercent)}%`,
      }).show();
      notifiedAboutSession = true;
    }
  }

  tray?.updateSnapshot(snapshot);
}
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
// src/shared/usageParser.test.ts
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
