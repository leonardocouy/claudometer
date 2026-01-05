import { BrowserWindow, ipcMain } from 'electron';
import type { ClaudeUsageSnapshot } from '../shared/claudeUsage.ts';
import type { ClaudeOrganization } from './claudeWebUsageClient.ts';

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

export class SettingsWindow {
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

    this.window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(getSettingsHtml())}`);
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

function getSettingsHtml(): string {
  return `
<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'self' 'unsafe-inline' data:; connect-src 'self' data:;" />
    <title>Claudometer Settings</title>
    <style>
      body { font-family: system-ui, -apple-system, Segoe UI, Roboto, sans-serif; margin: 16px; background: #0f1115; color: #e6e8ee; }
      h1 { font-size: 16px; margin: 0 0 12px; }
      .row { margin: 12px 0; }
      label { display: block; font-size: 12px; color: #a9b0c0; margin-bottom: 6px; }
      input[type="password"], input[type="number"], select { width: 100%; padding: 10px; border-radius: 8px; border: 1px solid #2a2f3a; background: #151923; color: #e6e8ee; }
      .hint { font-size: 12px; color: #a9b0c0; }
      .inline { display: flex; gap: 12px; align-items: center; }
      .inline > * { flex: 1; }
      .buttons { display: flex; gap: 10px; margin-top: 14px; }
      button { padding: 10px 12px; border-radius: 8px; border: 1px solid #2a2f3a; background: #1b2130; color: #e6e8ee; cursor: pointer; }
      button.primary { background: #2b6cff; border-color: #2b6cff; }
      button.danger { background: #2a1414; border-color: #7a2c2c; }
      button:disabled { opacity: 0.6; cursor: not-allowed; }
      .status { margin-top: 12px; padding: 10px; border-radius: 8px; background: #151923; border: 1px solid #2a2f3a; font-size: 12px; }
      .status strong { color: #fff; }
      .error { color: #ffb4b4; }
    </style>
  </head>
  <body>
    <h1>Claudometer</h1>

    <div class="row">
      <label for="sessionKey">Claude session key (from claude.ai cookies)</label>
      <input id="sessionKey" type="password" placeholder="sk-ant-sid01-..." autocomplete="off" />
      <div class="hint">Never paste this anywhere else. It is stored only if “Remember” is enabled.</div>
    </div>

    <div class="row inline">
      <div>
        <label for="rememberKey">Remember session key (OS credential storage)</label>
        <select id="rememberKey">
          <option value="false">No (memory only)</option>
          <option value="true">Yes</option>
        </select>
        <div class="hint" id="keytarHint"></div>
      </div>
      <div>
        <label for="refreshInterval">Refresh interval (seconds)</label>
        <input id="refreshInterval" type="number" min="10" step="1" />
      </div>
    </div>

    <div class="row">
      <label for="orgSelect">Organization</label>
      <select id="orgSelect"></select>
      <div class="hint">If empty, save a valid key and click Refresh.</div>
    </div>

    <div class="buttons">
      <button id="refreshNow">Refresh now</button>
      <button id="forgetKey" class="danger">Forget key</button>
      <button id="save" class="primary">Save</button>
    </div>

    <div class="status" id="statusBox">Loading…</div>

    <script>
      const { ipcRenderer } = require('electron');

      const el = (id) => document.getElementById(id);
      const sessionKeyEl = el('sessionKey');
      const rememberKeyEl = el('rememberKey');
      const refreshIntervalEl = el('refreshInterval');
      const orgSelectEl = el('orgSelect');
      const statusBoxEl = el('statusBox');
      const keytarHintEl = el('keytarHint');

      function setStatus(html) { statusBoxEl.innerHTML = html; }

      function renderOrgs(orgs, selectedId) {
        orgSelectEl.innerHTML = '';
        const emptyOpt = document.createElement('option');
        emptyOpt.value = '';
        emptyOpt.textContent = '(auto)';
        orgSelectEl.appendChild(emptyOpt);

        for (const org of orgs) {
          const opt = document.createElement('option');
          opt.value = org.id;
          opt.textContent = org.name ? \`\${org.name} (\${org.id})\` : org.id;
          orgSelectEl.appendChild(opt);
        }
        orgSelectEl.value = selectedId || '';
      }

      function renderSnapshot(snapshot) {
        if (!snapshot) return '<strong>Status:</strong> no data';
        if (snapshot.status !== 'ok') {
          const msg = snapshot.errorMessage ? \`<div class="error">\${snapshot.errorMessage}</div>\` : '';
          return \`<strong>Status:</strong> \${snapshot.status}\${msg}<div>Last updated: \${snapshot.lastUpdatedAt}</div>\`;
        }
        return \`
          <strong>Status:</strong> ok<br/>
          Session: \${Math.round(snapshot.sessionPercent)}%<br/>
          Weekly: \${Math.round(snapshot.weeklyPercent)}%<br/>
          \${snapshot.modelWeeklyName || 'Model'} (weekly): \${Math.round(snapshot.modelWeeklyPercent)}%\${snapshot.modelWeeklyResetsAt ? ' (resets ' + new Date(snapshot.modelWeeklyResetsAt).toLocaleString() + ')' : ''}<br/>
          Last updated: \${snapshot.lastUpdatedAt}
        \`;
      }

      async function loadState() {
        const state = await ipcRenderer.invoke('settings:getState');
        rememberKeyEl.value = String(Boolean(state.rememberSessionKey));
        refreshIntervalEl.value = String(state.refreshIntervalSeconds || 60);
        renderOrgs(state.organizations || [], state.selectedOrganizationId);
        keytarHintEl.textContent = state.keytarAvailable
          ? ''
          : 'On Linux, "Remember" saves to ~/.claudometer/session-key (chmod 600).';
        setStatus(renderSnapshot(state.latestSnapshot));
      }

      el('refreshNow').addEventListener('click', async () => {
        await ipcRenderer.invoke('settings:refreshNow');
        await loadState();
      });

      el('forgetKey').addEventListener('click', async () => {
        await ipcRenderer.invoke('settings:forgetKey');
        sessionKeyEl.value = '';
        await loadState();
      });

      el('save').addEventListener('click', async () => {
        const payload = {
          sessionKey: sessionKeyEl.value,
          rememberSessionKey: rememberKeyEl.value === 'true',
          refreshIntervalSeconds: Number(refreshIntervalEl.value || 60),
          selectedOrganizationId: orgSelectEl.value || undefined,
        };
        const result = await ipcRenderer.invoke('settings:save', payload);
        if (!result.ok) {
          setStatus('<strong>Status:</strong> <span class="error">error</span><div class="error">' + result.error + '</div>');
          return;
        }
        sessionKeyEl.value = '';
        await loadState();
      });

      loadState();
    </script>
  </body>
  </html>
  `;
}
