# Change: Refactor Claudometer from Electron to Tauri (tray-first) with autostart + updater

## Why
Claudometer is a tray-first app with a simple UI and lightweight background polling. Migrating from Electron to Tauri should materially reduce bundle size and runtime footprint on macOS + Linux while preserving the same product behavior.

We also need two production capabilities that are best supported with a Tauri-style release pipeline:
- **Auto-start** at login (user-controlled)
- **Secure updater** with signed releases

## What Changes
- **BREAKING (internal/runtime)**: Replace Electron main/preload processes with a Tauri (Rust) backend and system WebView renderer(s).
- Re-implement the tray-first lifecycle in Tauri v2 (no primary window on startup; settings window opened on demand).
- Add **autostart toggle** (macOS LaunchAgent + Linux autostart) using `tauri-plugin-autostart`.
- Add **updater** using `tauri-plugin-updater` + signed update artifacts and a `latest.json` manifest hosted in GitHub Releases.
- Update CI/release workflows to build signed artifacts for macOS + Linux and publish `latest.json`, following the approach used by:
  - https://github.com/pilgrimlyieu/Focust (tag-driven build, updater signing, `latest.json`)
  - https://github.com/RiteshK-611/LiveLayer (autostart plugin patterns)
- After the migration is complete, delete Electron-related code, tooling, and configs to keep the repository clean.

## Scope
- Platforms: macOS + Linux only (GNOME + KDE are the supported Linux desktop environments).
- Keep the application feature-set functionally equivalent to the current Electron app:
  - Poll and display Claude web usage (`five_hour`, `seven_day`, `seven_day_opus`).
  - Small settings UI for session key + refresh interval + org selection.
  - Existing “safe” behaviors: no sessionKey logs; graceful unauthorized/error states.
- Add: autostart + updater (new user-visible settings and tray menu actions).

## Out of Scope
- Windows support.
- Large UI redesign (keep settings window minimal).
- Additional integrations beyond current Claude usage fetching.

## Impact
- Affected specs:
  - `desktop-tray-app` (tray-first runtime changes)
  - `session-key-storage` (Electron `safeStorage` replacement)
  - `desktop-autostart` (new)
  - `desktop-updater` (new)
  - `release-pipeline` (new)
- Affected code:
  - Replace `src/main.ts`, `src/main/**`, `src/preload/**` and Electron Forge config with a Tauri project (`src-tauri/**`) plus a Vite-built settings renderer.
  - GitHub workflows: replace Electron Forge publish flow with a signed Tauri release flow.

## Decisions (confirmed)
1. **Release workflow**: **Option B** (keep Release Please) and adapt its build-and-publish job to build Tauri artifacts, generate signatures, and upload `latest.json` + artifacts to the GitHub Release.
2. **Update UX**: check on startup **and** provide a tray action “Check for Updates…”.
3. **Linux artifacts**: ship `.AppImage` and (if feasible without major complexity) also `.deb` + `.rpm`.
4. **Key storage**: OS Keychain/Secret Service only; no fallback file store.
