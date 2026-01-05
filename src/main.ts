/**
 * Claudometer - Electron Main Process (tray-first)
 */

import { app, Menu } from 'electron';
import { AppController } from './main/app-controller.ts';
import { registerIpcHandlers } from './main/ipc/register.ts';
import { ClaudeApiService } from './main/services/claude-api.ts';
import { SessionKeyService } from './main/services/session-key.ts';
import { SettingsService } from './main/services/settings.ts';
import { TrayService } from './main/tray.ts';
import { SettingsWindowService } from './main/windows/settings-window.ts';

let tray: TrayService | null = null;
let controller: AppController | null = null;
let settingsWindow: SettingsWindowService | null = null;

function openSettings(): void {
  settingsWindow ??= new SettingsWindowService();
  settingsWindow.show();
}

async function initialize(): Promise<void> {
  if (process.platform !== 'darwin') {
    Menu.setApplicationMenu(null);
  }

  if (process.platform === 'darwin') {
    app.dock?.hide();
  }

  settingsWindow = new SettingsWindowService();

  const settingsService = new SettingsService();
  const sessionKeyService = new SessionKeyService(settingsService);
  await sessionKeyService.migrateLegacyPlaintextIfNeeded();
  const claudeApiService = new ClaudeApiService();

  tray = new TrayService({
    onOpenSettings: openSettings,
    onRefreshNow: () => void controller?.refreshNow(),
    onQuit: () => app.quit(),
  });

  controller = new AppController({
    settingsService,
    sessionKeyService,
    claudeApiService,
    trayService: tray,
  });

  controller.onSnapshotUpdated((snapshot) => {
    settingsWindow?.sendSnapshotUpdated(snapshot);
  });

  registerIpcHandlers(controller);
  controller.start();
}

// Prevent multiple instances
const gotTheLock = app.requestSingleInstanceLock();
if (!gotTheLock) {
  app.quit();
}

app.whenReady().then(() => void initialize());
app.on('window-all-closed', () => {
  // Keep running in tray.
});

app.on('will-quit', () => {
  controller?.stop();
  tray?.destroy();
  tray = null;
});
