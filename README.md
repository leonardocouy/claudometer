# Claudometer

A tray-first desktop application for **macOS** and **Linux** that shows your Claude web usage limits in near real-time.

## Installation

### Download Pre-built Releases

Download the latest release for your platform:

**[Download from GitHub Releases](https://github.com/leonardocouy/claudometer/releases)**

| Platform | File |
|----------|------|
| macOS (Apple Silicon) | `Claudometer-x.x.x-arm64.dmg` |
| macOS (Intel) | `Claudometer-x.x.x-x64.dmg` |
| Linux (Debian/Ubuntu) | `claudometer_x.x.x_amd64.deb` |
| Linux (Universal) | `claudometer-x.x.x-x64.zip` |

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
   bun run make
   # Output in ./out/make/
   ```

## What This Does

Monitors your Claude.ai usage and displays it in your system tray:
- **5-hour session utilization** - How much you've used in the current rolling 5-hour window
- **Weekly utilization** - Your overall weekly Claude usage
- **Model-specific weekly usage** - Weekly limits for specific models (Opus, Sonnet, etc.)

The app polls Claude's web API at configurable intervals and updates the tray icon color based on your usage status.

## Quick Start (Development)

1. **Install dependencies**
   ```bash
   bun install
   ```

2. **Get your Claude session key**
   - Log in to [claude.ai](https://claude.ai)
   - Open DevTools (F12 or Cmd+Option+I)
   - Go to **Application** → **Cookies** → `https://claude.ai`
   - Copy the `sessionKey` value

3. **Run in development mode**
   ```bash
   bun run start
   ```

4. **Configure the app**
   - Click the tray icon → **"Open Settings..."**
   - Paste your session key
   - Set refresh interval (default: 60s)
   - Save

The tray will now show your Claude usage stats.

## Features

| Feature | Description |
|---------|-------------|
| **System Tray** | Lives in your menu bar/system tray - always visible |
| **Real-time Updates** | Configurable polling (minimum 10 seconds) |
| **Multi-organization** | Supports accounts with multiple Claude orgs |
| **Secure Storage** | Session key stored in OS keychain (macOS) or `~/.claudometer/session-key` on Linux when "Remember" is enabled |
| **Status Indicators** | Tray icon changes color based on status (green=ok, red=unauthorized, orange=rate limited) |
| **Auto-recovery** | Backs off automatically when rate-limited |

## Project Structure

```
claudometer/
├── src/
│   ├── main.ts                    # Electron main process entry (tray-first)
│   ├── main/                      # Main process modules
│   │   ├── tray.ts                # System tray icon and menu
│   │   ├── app-controller.ts      # Polling + state (single-flight setTimeout loop)
│   │   ├── ipc/                   # ipcMain handlers (settings actions)
│   │   ├── services/              # Claude API + settings + session key
│   │   └── windows/               # Settings window + push events
│   ├── preload/                   # contextBridge: exposes window.api
│   ├── renderer/                  # Vite renderer(s) for windows (settings)
│   └── common/                    # Shared types + parser + IPC contract
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
│ • Provide session key                                       │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ Main Process (src/main.ts)                                 │
│ • Initializes tray icon                                     │
│ • Starts polling timer (configurable interval)              │
│ • Coordinates data flow                                     │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ Polling Loop                                                │
│ 1. Fetch organizations (if needed)                          │
│ 2. Fetch usage snapshot for selected org                    │
│ 3. Parse JSON response                                      │
│ 4. Update tray icon and menu                                │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ Claude Web API Client                                       │
│ • GET /api/organizations                                    │
│ • GET /api/organizations/:id/usage                          │
│ • Includes sessionKey in Cookie header                      │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **App starts** → Loads saved session key from OS keychain (macOS) or `~/.claudometer/session-key` (Linux) if present
2. **Every N seconds** → Polls Claude API for usage data
3. **On response** → Parses JSON, updates tray icon color and menu text
4. **On error** → Updates tray to show error state, stops polling if unauthorized (401/403)

## Development

### Prerequisites

- [Bun](https://bun.sh) runtime
- macOS or Linux (Windows not currently supported)

### Available Scripts

| Command | Description |
|---------|-------------|
| `bun run start` | Run app in development mode with hot reload |
| `bun run package` | Package app (no distributable) |
| `bun run make` | Build distributables (.dmg, .deb, .zip) |
| `bun run publish` | Build and publish to GitHub Releases |
| `bun test` | Run unit tests |
| `bun run typecheck` | TypeScript type checking |
| `bun run check` | Run Biome linter and formatter checks |
| `bun run lint` | Auto-fix linting issues |
| `bun run format` | Auto-format code |

### Tech Stack

| Layer | Technology |
|-------|-----------|
| App Framework | Electron 39 |
| Build Tool | Electron Forge + Vite |
| Language | TypeScript 5.9 |
| Runtime | Bun |
| Settings Storage | `electron-store` (non-sensitive data) |
| Secret Storage | `keytar` (macOS only - session key) |
| Linting/Formatting | Biome |
| Testing | Bun's built-in test runner |

## Security & Privacy

### Session Key Handling

- **macOS**: Stored in system Keychain via `keytar` library
- **Linux**: If "Remember" is enabled, stored in `~/.claudometer/session-key` (chmod 600)
- **Never logged**: Session key is never included in logs, error messages, or telemetry
- **Validation before storage**: Session key is validated against Claude API before being saved

### What Gets Sent to Claude

Only standard HTTPS requests to `claude.ai/api/*` endpoints:
- `GET /api/organizations` - Fetch available organizations
- `GET /api/organizations/:id/usage` - Fetch usage stats

Your session key is sent as a Cookie header (same as when using Claude in a browser).

### Local Storage

The app stores these settings locally via `electron-store`:
- Refresh interval (seconds)
- Selected organization ID
- "Remember session key" preference

## Troubleshooting

### Tray shows "unauthorized"

Your session key is invalid or expired:
1. Open Settings
2. Get a fresh session key from claude.ai (see Quick Start)
3. Paste and save

### Tray shows "rate limited"

Claude API is rate-limiting your requests:
- The app automatically backs off for 5 minutes
- Consider increasing your refresh interval in Settings

### App won't start on Linux

Keytar dependency is optional on Linux. If you see errors related to `keytar`:
1. The app uses `~/.claudometer/session-key` when "Remember" is enabled (chmod 600)
2. If you leave "Remember" off, you'll need to re-enter the key after restarting

### No organizations found

Your Claude account doesn't have any organizations:
- Free Claude accounts still have a "personal" organization
- If you see this error, try logging out and back in to claude.ai
- Get a fresh session key

### Polling stopped

Check the tray menu:
- **Unauthorized**: Session key expired (see above)
- **Rate limited**: Auto-recovers in 5 minutes
- **Error**: Check the error message in the tray menu

## Roadmap

- [ ] Windows support
- [ ] Persistent session key storage on Linux (via libsecret)
- [ ] Desktop notifications when approaching usage limits
- [ ] Historical usage graphs
- [ ] Menu bar percentage display
- [ ] Auto-update mechanism

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
- [Electron](https://www.electronjs.org/) - Cross-platform desktop apps with web technologies
- [electron-store](https://github.com/sindresorhus/electron-store) - Simple settings persistence
