# Claudometer

A tray-first desktop application for **macOS** and **Linux** that shows your Claude web usage limits in near real-time.

## Installation

### Download Pre-built Releases

Download the latest release for your platform:

**[Download from GitHub Releases](https://github.com/leonardocouy/claudometer/releases)**

| Platform | File |
|----------|------|
| macOS (Apple Silicon / Intel) | `*.dmg` |
| Linux (GNOME/KDE) | `*.AppImage` (recommended), `*.deb`, `*.rpm` |

### Build from Source

1. **Install Bun**
   ```bash
   curl -fsSL https://bun.sh/install | bash
   ```

2. **Clone and install dependencies**
   ```bash
   git clone https://github.com/leonardocouy/claudometer.git
   cd claudometer
   bun install
   ```

3. **Build distributables**
   ```bash
   bun run build
   # Output in ./src-tauri/target/release/bundle/
   ```

## What This Does

Monitors your Claude usage and displays it in your system tray:
- **5-hour session utilization** - How much you've used in the current rolling 5-hour window
- **Weekly utilization** - Your overall weekly Claude usage
- **Model-specific weekly usage** - Weekly limits for specific models (Opus, Sonnet, etc.)

The app polls Claude's API at configurable intervals and updates the tray icon color based on your usage status.

## Authentication Modes

Claudometer supports **two authentication modes** - choose the one that fits your workflow:

### 🌐 Web Mode (Default)
Uses your Claude web session cookie (`sessionKey`) to access Claude.ai's web API.
- **Best for**: Regular Claude web users
- **Setup**: Extract session key from browser cookies (see Quick Start)
- **Pros**: Works immediately, no additional tools needed
- **Cons**: Session keys expire periodically (need to refresh)

### 🔧 CLI Mode (OAuth)
Uses Claude Code CLI OAuth credentials to access the Anthropic API directly.
- **Best for**: Claude Code CLI users, automation, long-lived sessions
- **Setup**: Authenticate once with `claude` CLI (credentials stored in `~/.claude/.credentials.json`)
- **Pros**: No manual session key extraction, tokens refresh automatically
- **Cons**: Requires Claude Code CLI installed and authenticated

Both modes track the same metrics and provide identical functionality. You can switch between modes anytime in Settings.

## Quick Start (Development)

1. **Install dependencies**
   ```bash
   bun install
   ```

2. **Choose your authentication mode**

   ### Option A: Web Mode (Session Key)
   - Log in to [claude.ai](https://claude.ai)
   - Open DevTools (F12 or Cmd+Option+I)
   - Go to **Application** → **Cookies** → `https://claude.ai`
   - Copy the `sessionKey` value

   ### Option B: CLI Mode (OAuth) - Recommended
   - Install Claude Code CLI: https://docs.anthropic.com/en/docs/agent-code
   - Authenticate once:
     ```bash
     claude
     # Follow OAuth flow in browser
     ```
   - Credentials saved to `~/.claude/.credentials.json` automatically

3. **Run in development mode**
   ```bash
   bun run dev
   ```

4. **Configure the app**
   - Click the tray icon → **"Open Settings..."**
   - **Web mode**: Select "Claude Web" and paste your session key
   - **CLI mode**: Select "Claude Code CLI" (no additional input needed)
   - Set refresh interval (default: 60s)
   - Save

The tray will now show your Claude usage stats.

## Features

| Feature | Description |
|---------|-------------|
| **Dual Authentication** | Web mode (session key) or CLI mode (OAuth API) - your choice |
| **System Tray** | Lives in your menu bar/system tray - always visible |
| **Real-time Updates** | Configurable polling (minimum 10 seconds) |
| **Multi-organization** | Supports accounts with multiple Claude orgs |
| **Secure Storage** | Session key is stored only in OS Keychain/Secret Service (or kept in memory if “Remember” is disabled) |
| **Status Indicators** | Tray icon changes color based on status (green=ok, red=unauthorized, orange=rate limited) |
| **Auto-recovery** | Backs off automatically when rate-limited |
| **Updater** | Signed auto-updates via `latest.json` + `.sig` assets in GitHub Releases |

## Project Structure

```
claudometer/
├── src-tauri/                     # Tauri (Rust) backend + bundling config
│   ├── tauri.conf.json            # App + bundle + updater config
│   ├── capabilities/              # Permission scopes
│   └── src/                       # Rust modules (tray, polling, commands, settings)
├── src/
│   ├── renderer/settings/         # Vite settings UI (Tauri invoke + events)
│   └── common/                    # Shared types for the settings UI
├── assets/                        # Tray icons
├── openspec/                      # Change proposals & specs
├── package.json
├── tsconfig.json
└── CLAUDE.md                      # AI assistant instructions
```

## How It Works

```
┌─────────────────────────────────────────────────────────────┐
│ User Actions                                                │
│ • Launch app                                                │
│ • Open settings                                             │
│ • Select mode: Web (session key) OR CLI (OAuth)            │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ Tauri backend (src-tauri/src)                               │
│ • Initializes tray icon                                     │
│ • Starts polling loop (configurable interval)               │
│ • Coordinates data flow                                     │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ AppController - Dual Routing                                │
│ • Checks usageSource setting ('web' or 'cli')               │
│ • Routes to appropriate service                             │
└───────┬─────────────────────────────────────────────────────┘
        │
        ├──────────────────┬──────────────────────────────────┐
        │                  │                                  │
        ▼                  ▼                                  ▼
┌──────────────┐  ┌──────────────┐               ┌──────────────┐
│  WEB MODE    │  │  CLI MODE    │               │ Polling Loop │
│              │  │              │               │ (Either Mode)│
│ Claude Web   │  │ OAuth API    │               └──────────────┘
│ API Client   │  │ Client       │                      │
│              │  │              │                      ▼
│ • GET /api/  │  │ • GET oauth/ │               1. Fetch usage
│   orgs       │  │   usage      │               2. Parse JSON
│ • GET /api/  │  │ • Bearer     │               3. Update tray
│   orgs/:id/  │  │   token from │
│   usage      │  │   ~/.claude/ │
│ • Cookie:    │  │   .credentials│
│   sessionKey │  │              │
└──────────────┘  └──────────────┘
        │                  │
        └──────────┬───────┘
                   ▼
        ┌─────────────────────┐
        │ ClaudeUsageSnapshot │
        │ (unified format)    │
        └─────────────────────┘
```

### Data Flow

1. **App starts** → Loads session key from OS credential storage (if “Remember” is enabled)
2. **Every N seconds** → Polls Claude API for usage data
3. **On response** → Parses JSON, updates tray icon color and menu text
4. **On error** → Updates tray to show error state, stops polling if unauthorized (401/403)

**CLI Mode:**
1. **App starts** → Reads OAuth credentials from `~/.claude/.credentials.json`
2. **Every N seconds** → Calls Anthropic OAuth API with Bearer token
3. **On response** → Parses JSON (same format), updates tray
4. **On error** → Shows "re-authenticate with claude" if 401, continues polling otherwise

## Development

### Prerequisites

- [Bun](https://bun.sh) runtime
- macOS or Linux (Windows not currently supported)

### Available Scripts

| Command | Description |
|---------|-------------|
| `bun run dev` | Run Tauri app in development mode |
| `bun run dev:ui` | Run settings UI only (Vite) |
| `bun run build` | Build Tauri bundles (uses `tauri.conf.json`) |
| `bun run build:ui` | Build settings UI only |
| `bun run sync-versions` | Keep versions in sync across config files |
| `bun run typecheck` | TypeScript type checking |
| `bun run check` | Run Biome linter and formatter checks |
| `bun run lint` | Auto-fix linting issues |
| `bun run format` | Auto-format code |

### Tech Stack

| Layer | Technology |
|-------|-----------|
| App Framework | Tauri v2 |
| Build Tool | Tauri CLI + Vite |
| Language | TypeScript 5.9 |
| Runtime | Bun |
| Settings Storage | `tauri-plugin-store` (non-sensitive data) |
| Secret Storage | OS Keychain / Secret Service (`keyring` crate) |
| Linting/Formatting | Biome |
| Testing | Bun's built-in test runner |

## Manual Test Matrix

Run these checks on:
- macOS (Apple Silicon + Intel)
- Linux (GNOME + KDE)

Checklist:
1. Tray starts with no windows; menu shows snapshot lines.
2. “Open Settings…” creates/focuses the settings window.
3. Saving a valid session key refreshes snapshot and updates tray.
4. “Remember session key” persists across restart (Keychain / Secret Service).
5. Disabling “Remember” keeps the key memory-only (does not persist across restart).
6. Autostart toggle reflects system state after restart/login.
7. “Check for Updates…” shows a result (up-to-date / update available / error).

## Security & Privacy

### Authentication Handling

- **Stored only in OS credential storage** when “Remember” is enabled (Keychain / Secret Service)
- **Memory-only** when “Remember” is disabled (no persistence)
- **Never logged**: Session key is never included in logs, error messages, or telemetry
- **Validation before storage**: Session key is validated against Claude API before being saved

**CLI Mode:**
- **Managed by Claude CLI**: OAuth tokens stored in `~/.claude/.credentials.json` (managed by Claude Code CLI)
- **Auto-refresh**: Tokens are refreshed automatically by the CLI
- **Claudometer reads only**: App only reads credentials, never modifies them
- **No persistence**: Claudometer doesn't store or cache OAuth tokens

### What Gets Sent

**Web Mode:**
- HTTPS requests to `claude.ai/api/*` endpoints:
  - `GET /api/organizations` - Fetch available organizations
  - `GET /api/organizations/:id/usage` - Fetch usage stats
- Session key sent as Cookie header (same as browser)

**CLI Mode:**
- HTTPS requests to `api.anthropic.com/api/oauth/*` endpoints:
  - `GET /api/oauth/usage` - Fetch usage stats
- OAuth token sent as Bearer header

### Local Storage

The app stores these settings locally (non-sensitive) via `tauri-plugin-store`:
- Refresh interval (seconds)
- Selected organization ID
- "Remember session key" preference
 - Autostart preference
 - Updater preferences

## Troubleshooting

### Tray shows "unauthorized"

**Web Mode:**
Your session key is invalid or expired:
1. Open Settings
2. Get a fresh session key from claude.ai (see Quick Start)
3. Paste and save

**CLI Mode:**
Your OAuth token expired:
1. Re-authenticate with Claude Code CLI:
   ```bash
   claude
   # Follow OAuth flow again
   ```
2. App will automatically use new credentials

### Tray shows "rate limited"

Claude API is rate-limiting your requests:
- The app automatically backs off for 5 minutes
- Consider increasing your refresh interval in Settings

### App won't start on Linux

If “Remember session key” is disabled in Settings, your session key will not persist across restarts.

### No organizations found

Your Claude account doesn't have any organizations:
- Free Claude accounts still have a "personal" organization
- If you see this error, try logging out and back in to claude.ai
- Get a fresh session key

### Polling stopped

Check the tray menu:
- **Unauthorized**: Session key/token expired (see above)
- **Rate limited**: Auto-recovers in 5 minutes
- **Error**: Check the error message in the tray menu

### CLI mode not working

If you selected "Claude Code CLI" but see "No OAuth credentials found":
1. **Check credentials file exists**:
   ```bash
   ls -la ~/.claude/.credentials.json
   ```
2. **If missing, authenticate**:
   ```bash
   claude
   # Follow OAuth flow in browser
   ```
3. **Check file permissions**:
   ```bash
   chmod 600 ~/.claude/.credentials.json
   ```
4. **Restart app** to reload credentials

## Roadmap

- [ ] Windows support
- [ ] Desktop notifications when approaching usage limits
- [ ] Historical usage graphs
- [ ] Menu bar percentage display
- [x] Auto-update mechanism

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes following the existing code style
4. Run `bun run check` to ensure code quality
5. Submit a pull request

## License

MIT

## Related Projects

- [Claude API](https://docs.anthropic.com/claude/reference/getting-started-with-the-api) - Official API (different from web usage tracking)
- [Tauri](https://tauri.app/) - Lightweight desktop apps with Rust backend + system WebView
