import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { BrowserWindow, ipcMain } from 'electron';
import type { ClaudeUsageSnapshot } from '../../core/types.ts';
import type { ClaudeOrganization } from '../../services/claude-api.ts';

export type SettingsState = {
  rememberSessionKey: boolean;
  refreshIntervalSeconds: number;
  organizations: ClaudeOrganization[];
  selectedOrganizationId?: string;
  latestSnapshot: ClaudeUsageSnapshot | null;
  keytarAvailable: boolean;
};

export type SaveSettingsPayload = {
  sessionKey: string;
  rememberSessionKey: boolean;
  refreshIntervalSeconds: number;
  selectedOrganizationId?: string;
};

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export class SettingsWindowService {
  private window: BrowserWindow | null = null;
  private getState: () => Promise<SettingsState>;
  private onSave: (
    payload: SaveSettingsPayload,
  ) => Promise<{ ok: true } | { ok: false; error: string }>;
  private onForgetKey: () => Promise<void>;
  private onRefreshNow: () => Promise<void>;

  constructor(options: {
    getState: () => Promise<SettingsState>;
    onSave: (payload: SaveSettingsPayload) => Promise<{ ok: true } | { ok: false; error: string }>;
    onForgetKey: () => Promise<void>;
    onRefreshNow: () => Promise<void>;
  }) {
    this.getState = options.getState;
    this.onSave = options.onSave;
    this.onForgetKey = options.onForgetKey;
    this.onRefreshNow = options.onRefreshNow;
  }

  show(): void {
    if (this.window) {
      this.window.show();
      this.window.focus();
      return;
    }

    this.window = new BrowserWindow({
      width: 520,
      height: 620,
      resizable: false,
      minimizable: false,
      maximizable: false,
      fullscreenable: false,
      title: 'Claudometer Settings',
      autoHideMenuBar: true,
      backgroundColor: '#0f1115',
      webPreferences: {
        nodeIntegration: true,
        contextIsolation: false,
      },
    });

    // After bundling, __dirname points to .vite/build/ where main.js lives
    // The HTML is copied to .vite/build/ui/settings-window/ by vite-plugin-static-copy
    const htmlPath = path.join(__dirname, 'ui', 'settings-window', 'settings.html');
    this.window.loadFile(htmlPath);
    this.window.setMenuBarVisibility(false);
    this.window.on('closed', () => {
      this.cleanupIpc();
      this.window = null;
    });

    this.setupIpc();
  }

  private setupIpc(): void {
    ipcMain.handle('settings:getState', async () => this.getState());
    ipcMain.handle('settings:save', async (_event, payload: SaveSettingsPayload) =>
      this.onSave(payload),
    );
    ipcMain.handle('settings:forgetKey', async () => {
      await this.onForgetKey();
      return { ok: true } as const;
    });
    ipcMain.handle('settings:refreshNow', async () => {
      await this.onRefreshNow();
      return { ok: true } as const;
    });
  }

  private cleanupIpc(): void {
    ipcMain.removeHandler('settings:getState');
    ipcMain.removeHandler('settings:save');
    ipcMain.removeHandler('settings:forgetKey');
    ipcMain.removeHandler('settings:refreshNow');
  }
}
