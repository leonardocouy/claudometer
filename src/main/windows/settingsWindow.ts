import path from 'node:path';
import { BrowserWindow, shell } from 'electron';
import { ipcChannels } from '../../common/ipc.ts';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';

declare const SETTINGS_VITE_DEV_SERVER_URL: string | undefined;
declare const SETTINGS_VITE_NAME: string;

export class SettingsWindowService {
  private window: BrowserWindow | null = null;

  show(): void {
    if (this.window) {
      this.window.show();
      this.window.focus();
      return;
    }

    this.window = new BrowserWindow({
      width: 540,
      height: 620,
      resizable: false,
      minimizable: false,
      maximizable: false,
      fullscreenable: false,
      title: 'Claudometer Settings',
      autoHideMenuBar: true,
      backgroundColor: '#0f1115',
      webPreferences: {
        nodeIntegration: false,
        contextIsolation: true,
        preload: path.join(__dirname, 'preload.js'),
      },
    });

    // Open external links in system browser
    this.window.webContents.setWindowOpenHandler(({ url }) => {
      void shell.openExternal(url);
      return { action: 'deny' };
    });

    if (SETTINGS_VITE_DEV_SERVER_URL) {
      void this.window.loadURL(SETTINGS_VITE_DEV_SERVER_URL);
    } else {
      const htmlPath = path.join(__dirname, `../renderer/${SETTINGS_VITE_NAME}/index.html`);
      void this.window.loadFile(htmlPath);
    }

    this.window.setMenuBarVisibility(false);
    this.window.webContents.openDevTools(); // Open DevTools automatically
    this.window.on('closed', () => {
      this.window = null;
    });
  }

  sendSnapshotUpdated(snapshot: ClaudeUsageSnapshot | null): void {
    this.window?.webContents.send(ipcChannels.events.snapshotUpdated, snapshot);
  }
}
