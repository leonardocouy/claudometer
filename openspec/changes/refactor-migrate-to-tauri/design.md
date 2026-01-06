# Design: Tauri migration (tray-first) + autostart + updater

## Goals
- Preserve Claudometer’s tray-first UX on macOS + Linux.
- Reduce bundle size and baseline resource usage by moving from Electron to Tauri.
- Add:
  - Autostart (user-controlled)
  - Signed updater using GitHub Releases (`latest.json`)
- Preserve security/privacy constraints:
  - Never log or persist the Claude `sessionKey` outside OS credential storage.
  - Never include the session key in UI strings or errors.

## Tauri Version & Reference Pattern
Target Tauri **v2** and follow the project structure/patterns seen in:
- Focust (`src-tauri/`, `capabilities/`, `tauri-plugin-updater`, `tauri-plugin-autostart`, tag-based release, `latest.json`)
- LiveLayer (command patterns for autostart toggling)

Key config choices (modeled after Focust):
- `src-tauri/tauri.conf.json` with:
  - `"bundle.createUpdaterArtifacts": true`
  - `"plugins.updater.endpoints": ["https://github.com/<owner>/<repo>/releases/latest/download/latest.json"]`
  - `"plugins.updater.pubkey": "<minisign pubkey>"`
- `src-tauri/capabilities/default.json` allowing only the required permissions/plugins for the settings window(s).

## Runtime Architecture

### High-level shape
- **Rust backend (Tauri)** owns:
  - app lifecycle
  - tray icon + menu rendering
  - polling loop and backoff
  - secure session key storage (keychain / Secret Service)
  - settings persistence (non-sensitive)
  - updater + update check/install prompts
- **Settings window renderer (Vite)** owns:
  - a minimal UI (similar to current settings window)
  - invokes Rust commands to read/update settings and session key
  - displays the latest snapshot pushed from backend events

### Tray-first lifecycle
- Start with **no visible windows** (Tauri config `app.windows: []`).
- Create the tray icon immediately.
- Create/open the settings window only when:
  - user clicks “Open Settings…” in tray menu, or
  - app receives a second-instance launch (focus settings), or
  - user needs to configure a missing/expired session key (tray shows status; settings action available).

### Polling & state
- Keep a single-flight polling loop with jittered backoff on 429 and stop-on-unauthorized behavior (matching current Electron behavior).
- Store the latest snapshot in Rust memory and:
  - update tray menu on every snapshot update
  - emit an event to settings window(s) so the UI stays in sync

### Settings persistence (non-sensitive)
Options (decision during implementation):
- Use `tauri-plugin-store` for `refreshIntervalSeconds`, `selectedOrganizationId`, `rememberSessionKey`, `autostartEnabled`, `checkUpdatesOnStartup`.
- Or implement a small JSON settings file in `app_config_dir`.

### Session key storage (sensitive)
Replace Electron `safeStorage` with:
- OS credential storage via a Rust keyring solution (Keychain on macOS; Secret Service on Linux).
- Store only non-sensitive flags in app settings.
- Provide explicit “Forget session key” operation.

Fallback behavior:
- If OS credential APIs are not available, either:
  - deny “remember key” and run memory-only, or
  - use a fallback encrypted file store (requires a clear security story).

## Autostart
Implement with `tauri-plugin-autostart`:
- macOS: `MacosLauncher::LaunchAgent`
- Linux: plugin-managed autolaunch (desktop entry / XDG autostart)

UX:
- Settings toggle “Start at login”
- Tray menu shortcut (optional)
- Ensure enabling/disabling autostart updates both system state and stored preference.

## Updater
Implement with `tauri-plugin-updater` using the “latest.json in GitHub Releases” pattern from Focust.

### Signing
- Use Tauri signer key pair:
  - Public key committed in `tauri.conf.json`
  - Private key in GitHub Actions secrets:
    - `TAURI_SIGNING_PRIVATE_KEY`
    - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (optional)
- Ensure CI builds generate `.sig` files and `latest.json` includes signatures.

### `latest.json` generation
Follow Focust’s release workflow:
- Build artifacts for macOS + Linux
- Collect:
  - Linux: `.AppImage` (and optionally `.deb`, `.rpm`)
  - macOS: `.app.tar.gz` + `.dmg` (choose one as update payload; Focust uses `.app.tar.gz`)
- Generate `latest.json` with:
  - `version`: `vX.Y.Z`
  - `notes`: release URL
  - `pub_date`: UTC ISO timestamp
  - `platforms`: `linux-x86_64`, `darwin-x86_64`, `darwin-aarch64`

## CI / Release Workflow
Selected approach: keep Release Please (Option B).

### Option A (recommended): Tag-driven release (Focust approach)
- Add `scripts/release.ts` (Bun) to:
  - bump versions in `package.json` + `src-tauri/tauri.conf.json`
  - update `CHANGELOG.md` from `RELEASE_NOTE.md`
  - commit + create `vX.Y.Z` tag + push
- GitHub Actions workflow triggers on `v*.*.*` tags:
  - build with Bun + Rust toolchain
  - install Linux build deps: `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `libxss-dev`
  - produce artifacts and signatures
  - generate/upload `latest.json`
  - create GitHub Release

### Option B: Keep Release Please
- Release Please creates tag/release from main.
- Build job runs only when `release_created == true`:
  - build artifacts + signatures (macOS + Linux)
  - generate `latest.json`
  - upload artifacts + `latest.json` to the created GitHub Release

Notes for implementation (modeled after the Focust workflow):
- Ensure Linux CI installs WebKitGTK + AppIndicator build dependencies for tray support.
- Prefer building a macOS universal artifact where feasible and mapping it to `darwin-x86_64` and `darwin-aarch64` in `latest.json`.

## Risks / Gotchas
- Linux tray behavior varies by DE; GNOME may require AppIndicator support (build deps + user environment).
- Linux WebView dependencies must be available on target distros; document requirements for source builds.
- Updater requires stable `latest.json` URL and signing key hygiene (rotation has user impact).
