## 1. Proposal Decisions
- [x] 1.1 Release workflow: keep Release Please (Option B) and adapt build to Tauri + `latest.json`.
- [x] 1.2 Update UX: check on startup and also offer “Check for Updates…”.
- [x] 1.3 Linux artifacts: ship `.AppImage` + `.deb` + `.rpm` if feasible.
- [x] 1.4 Session key storage: no fallback; only OS keychain/Secret Service.

## 2. Tauri Scaffold
- [x] 2.1 Add Tauri v2 project skeleton (`src-tauri/`, `tauri.conf.json`, icons, capabilities).
- [x] 2.2 Wire front-end build (Vite) for the settings window.
- [x] 2.3 Add a minimal Rust module layout: tray, polling, settings, secure storage, updater.

## 3. Tray-first UX
- [x] 3.1 Implement tray icon + menu with current actions: Refresh now, Open Settings…, Quit.
- [x] 3.2 Render the latest usage snapshot in the menu (same fields as Electron app).
- [x] 3.3 Ensure app starts with no main window and persists in tray.

## 4. Claude Usage Fetching
- [x] 4.1 Port Claude API client behavior (org list + usage snapshot) into Rust backend commands/tasks.
- [x] 4.2 Port parsing/normalization logic and preserve error statuses (ok/unauthorized/rate_limited/error/missing_key).
- [x] 4.3 Implement the polling loop with backoff (rate limit) and stop-on-unauthorized behavior.

## 5. Settings + Secure Storage
- [x] 5.1 Persist non-sensitive settings (refresh interval, org ID, autostart, updater preferences).
- [x] 5.2 Implement OS credential storage for `sessionKey` (macOS Keychain + Linux Secret Service).
- [x] 5.3 Implement remember/forget flows and ensure no logs/UI can leak `sessionKey`.
- [x] 5.4 Update settings UI to use `invoke` commands and subscribe to snapshot events.

## 6. Autostart
- [x] 6.1 Add `tauri-plugin-autostart` and expose commands to enable/disable and query state.
- [x] 6.2 Add settings UI toggle and (optional) tray shortcut.
- [ ] 6.3 Validate on macOS and Linux (GNOME + KDE).

## 7. Updater
- [x] 7.1 Add `tauri-plugin-updater` and integrate a “Check for Updates…” tray action (and/or startup check).
- [x] 7.2 Add signing setup docs and helper script (modeled after Focust).
- [x] 7.3 Ensure release artifacts include `.sig` files and `latest.json`.

## 8. CI / Release Pipeline
- [x] 8.1 Replace Electron workflows with Tauri workflows (build, checks, release).
- [x] 8.2 Add Linux build dependencies to CI (webkit2gtk + libsoup + appindicator).
- [x] 8.3 Implement release packaging and `latest.json` generation (Focust pattern).

## 9. Documentation & Validation
- [x] 9.1 Update `README.md` and `ARCHITECTURE.md` to reflect Tauri structure.
- [x] 9.2 Manual test matrix: macOS (Intel+Apple Silicon) and Linux (GNOME + KDE).
- [ ] 9.3 Smoke test updater against a draft/test release.

## 10. Cleanup (Remove Electron)
- [x] 10.1 Remove Electron Forge config and scripts (`forge.config.ts`, Electron-related `package.json` scripts/deps).
- [x] 10.2 Remove Electron-only source folders (`src/main.ts`, `src/main/**`, `src/preload/**`) after Tauri equivalents exist.
- [x] 10.3 Remove Electron build outputs and related configs (Vite configs that only exist for Electron main/preload).
- [x] 10.4 Verify no lingering Electron references (`electron`, `electron-forge`, `safeStorage`) remain in the repo.
