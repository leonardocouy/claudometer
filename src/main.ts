/**
 * Claudometer - Electron Main Process (tray-first)
 */

import { app, Menu } from 'electron';
import { AppController } from './main/appController.ts';
import { registerIpcHandlers } from './main/ipc/register.ts';
import { ClaudeApiService } from './main/services/claudeApi.ts';
import { NotificationSoundService } from './main/services/notificationSound.ts';
import { SessionKeyService } from './main/services/sessionKey.ts';
import { SettingsService } from './main/services/settings.ts';
import { UsageNotificationService } from './main/services/usageNotification.ts';
import { TrayService } from './main/tray.ts';
import { SettingsWindowService } from './main/windows/settingsWindow.ts';

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
  const claudeApiService = new ClaudeApiService();
  const notificationSoundService = new NotificationSoundService({
    platform: process.platform,
    userDataDir: app.getPath('userData'),
  });
  const usageNotificationService = new UsageNotificationService(
    settingsService,
    notificationSoundService,
  );

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
    usageNotificationService,
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
