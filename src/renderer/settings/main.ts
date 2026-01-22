import './styles.css';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { IpcResult, SaveSettingsPayload, SettingsState } from '../../common/ipc.ts';
import type {
  ClaudeOrganization,
  CodexUsageSource,
  UsageSnapshotBundle,
  UsageSource,
} from '../../common/types.ts';

const el = <T extends HTMLElement>(root: ParentNode, selector: string): T => {
  const node = root.querySelector(selector);
  if (!node) throw new Error(`Missing element: ${selector}`);
  return node as T;
};

async function settingsGetState(): Promise<SettingsState> {
  return await invoke<SettingsState>('settings_get_state');
}

async function settingsSave(payload: SaveSettingsPayload): Promise<IpcResult<null>> {
  return await invoke<IpcResult<null>>('settings_save', { payload });
}

async function settingsForgetKey(): Promise<IpcResult<null>> {
  return await invoke<IpcResult<null>>('settings_forget_key');
}

async function settingsForgetClaudeKey(): Promise<IpcResult<null>> {
  return await invoke<IpcResult<null>>('settings_forget_claude_key');
}

async function settingsRefreshNow(): Promise<IpcResult<null>> {
  return await invoke<IpcResult<null>>('settings_refresh_now');
}

function renderOrgs(
  orgSelectEl: HTMLSelectElement,
  orgs: ClaudeOrganization[],
  selectedId?: string,
) {
  orgSelectEl.innerHTML = '';
  const emptyOpt = document.createElement('option');
  emptyOpt.value = '';
  emptyOpt.textContent = '(auto)';
  orgSelectEl.appendChild(emptyOpt);

  for (const org of orgs) {
    const opt = document.createElement('option');
    opt.value = org.id;
    opt.textContent = org.name ? `${org.name} (${org.id})` : org.id;
    orgSelectEl.appendChild(opt);
  }
  orgSelectEl.value = selectedId || '';
}

const escapeHtml = (value: string): string =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');

function renderSnapshot(snapshot: UsageSnapshotBundle | null): string {
  if (!snapshot) return '<strong>Status:</strong> no data';

  const renderClaude = (snap: UsageSnapshotBundle['claude']) => {
    if (!snap) return '<strong>Claude:</strong> no data';

    if (snap.status !== 'ok') {
      const msg = snap.errorMessage
        ? `<div class="error">${escapeHtml(snap.errorMessage)}</div>`
        : '';
      return `<strong>Claude:</strong> ${snap.status}${msg}<div>Last updated: ${escapeHtml(
        snap.lastUpdatedAt,
      )}</div>`;
    }

    const models = snap.models.length
      ? snap.models
          .map((m) => {
            const reset = m.resetsAt ? ` (resets ${new Date(m.resetsAt).toLocaleString()})` : '';
            return `${escapeHtml(m.name)} (weekly): ${Math.round(m.percent)}%${reset}`;
          })
          .join('<br/>')
      : 'Models (weekly): (none)';

    return `
      <strong>Claude:</strong> ok<br/>
      Session: ${Math.round(snap.sessionPercent)}%<br/>
      Weekly: ${Math.round(snap.weeklyPercent)}%<br/>
      ${models}<br/>
      Last updated: ${escapeHtml(snap.lastUpdatedAt)}
    `;
  };

  const renderCodex = (snap: UsageSnapshotBundle['codex']) => {
    if (!snap) return '<strong>Codex:</strong> no data';

    if (snap.status !== 'ok') {
      const msg = snap.errorMessage
        ? `<div class="error">${escapeHtml(snap.errorMessage)}</div>`
        : '';
      return `<strong>Codex:</strong> ${snap.status}${msg}<div>Last updated: ${escapeHtml(
        snap.lastUpdatedAt,
      )}</div>`;
    }

    return `
      <strong>Codex:</strong> ok<br/>
      Session: ${Math.round(snap.sessionPercent)}%<br/>
      Weekly: ${Math.round(snap.weeklyPercent)}%<br/>
      Models (weekly): (n/a)<br/>
      Last updated: ${escapeHtml(snap.lastUpdatedAt)}
    `;
  };

  return `${renderClaude(snapshot.claude)}<br/><br/>${renderCodex(snapshot.codex)}`;
}

function setStatus(statusBoxEl: HTMLElement, html: string): void {
  statusBoxEl.innerHTML = html;
}

function setResultError(statusBoxEl: HTMLElement, result: IpcResult<unknown>): void {
  if (result.ok) return;

  // Clear previous content
  statusBoxEl.textContent = '';

  // Status label
  const statusLabel = document.createElement('strong');
  statusLabel.textContent = 'Status: ';
  statusBoxEl.appendChild(statusLabel);

  // Error status
  const errorSpan = document.createElement('span');
  errorSpan.className = 'error';
  errorSpan.textContent = 'error';
  statusBoxEl.appendChild(errorSpan);

  // Error message
  statusBoxEl.appendChild(document.createElement('br'));
  const errorDiv = document.createElement('div');
  errorDiv.className = 'error';
  errorDiv.textContent = result.error.message; // textContent prevents XSS
  statusBoxEl.appendChild(errorDiv);
}

type Ui = {
  trackClaudeEl: HTMLInputElement;
  trackCodexEl: HTMLInputElement;
  claudeSectionEl: HTMLElement;
  codexSectionEl: HTMLElement;

  usageSourceEl: HTMLSelectElement;
  webOnlySectionEl: HTMLElement;
  cliHintEl: HTMLElement;
  sessionKeyEl: HTMLInputElement;
  rememberKeyEl: HTMLInputElement;
  claudeStorageHintEl: HTMLElement;
  orgSelectEl: HTMLSelectElement;

  codexUsageSourceEl: HTMLSelectElement;
  codexHintEl: HTMLElement;

  refreshIntervalEl: HTMLSelectElement;
  notifyResetEl: HTMLInputElement;
  autostartEl: HTMLInputElement;
  updatesStartupEl: HTMLInputElement;

  forgetKeyButton: HTMLButtonElement;
  forgetClaudeKeyButton: HTMLButtonElement;
  statusBoxEl: HTMLElement;
};

function applyVisibility(
  ui: Ui,
  trackClaudeEnabled: boolean,
  trackCodexEnabled: boolean,
  claudeSource: UsageSource,
  codexSource: CodexUsageSource,
) {
  ui.claudeSectionEl.toggleAttribute('hidden', !trackClaudeEnabled);
  ui.codexSectionEl.toggleAttribute('hidden', !trackCodexEnabled);

  ui.webOnlySectionEl.toggleAttribute('hidden', !trackClaudeEnabled || claudeSource !== 'web');
  ui.cliHintEl.toggleAttribute('hidden', !trackClaudeEnabled || claudeSource !== 'cli');

  ui.forgetKeyButton.toggleAttribute('hidden', true);
  ui.forgetClaudeKeyButton.toggleAttribute('hidden', !trackClaudeEnabled || claudeSource !== 'web');

  ui.codexHintEl.textContent =
    codexSource === 'cli'
      ? 'Uses the local codex CLI (no network).'
      : 'Uses your local Codex login (reads ~/.codex/auth.json).';
}

async function loadState(ui: Ui): Promise<SettingsState> {
  const state = await settingsGetState();
  ui.trackClaudeEl.checked = Boolean(state.trackClaudeEnabled);
  ui.trackCodexEl.checked = Boolean(state.trackCodexEnabled);
  ui.usageSourceEl.value = state.usageSource;
  ui.codexUsageSourceEl.value = state.codexUsageSource;
  applyVisibility(
    ui,
    state.trackClaudeEnabled,
    state.trackCodexEnabled,
    state.usageSource,
    state.codexUsageSource,
  );

  ui.rememberKeyEl.checked = Boolean(state.rememberSessionKey);
  ui.refreshIntervalEl.value = String(state.refreshIntervalSeconds || 60);
  ui.notifyResetEl.checked = state.notifyOnUsageReset ?? false;
  ui.autostartEl.checked = state.autostartEnabled ?? false;
  ui.updatesStartupEl.checked = state.checkUpdatesOnStartup ?? true;
  renderOrgs(ui.orgSelectEl, state.organizations || [], state.selectedOrganizationId);

  ui.rememberKeyEl.disabled = !state.keyringAvailable;
  if (!state.keyringAvailable) {
    ui.rememberKeyEl.checked = false;
  }
  ui.claudeStorageHintEl.textContent = state.keyringAvailable
    ? ''
    : 'OS keychain/secret service is unavailable. “Remember session key” is disabled on this system.';

  setStatus(ui.statusBoxEl, renderSnapshot(state.latestSnapshot));
  ui.sessionKeyEl.value = '';
  return state;
}

function renderApp(root: HTMLElement): void {
  root.innerHTML = `
    <h1>Claudometer</h1>

    <div class="row">
      <div class="setting">
        <div class="setting-text">
          <label class="setting-title" for="trackClaude">Track Claude</label>
          <div class="hint">Show Claude usage in the tray title.</div>
        </div>
        <input id="trackClaude" class="setting-checkbox" type="checkbox" />
      </div>
    </div>

    <div class="row">
      <div class="setting">
        <div class="setting-text">
          <label class="setting-title" for="trackCodex">Track Codex</label>
          <div class="hint">Show Codex usage in the tray title.</div>
        </div>
        <input id="trackCodex" class="setting-checkbox" type="checkbox" />
      </div>
    </div>

    <div id="claudeSection">
    <div class="row">
      <label for="usageSource">Usage data source</label>
      <select id="usageSource">
        <option value="cli">Claude Code</option>
        <option value="web">Claude Web (session key cookie)</option>
      </select>
      <div class="hint" id="cliHint" hidden>
        Uses your Claude Code login to fetch usage. If it isn't set up yet, run <code>claude login</code>.
      </div>
    </div>

    <div id="webOnlySection">
    <div class="row">
      <label for="sessionKey">Claude session key (from claude.ai cookies)</label>
      <input id="sessionKey" type="password" placeholder="sk-ant-sid01-..." autocomplete="off" />
      <div class="hint">Never paste this anywhere else. It is stored only if "Remember" is enabled.</div>
    </div>

    <div class="row">
      <div class="setting">
        <div class="setting-text">
          <label class="setting-title" for="rememberKey">Remember session key</label>
          <div class="hint" id="claudeStorageHint"></div>
        </div>
        <input id="rememberKey" class="setting-checkbox" type="checkbox" />
      </div>
    </div>

    <div class="row">
      <label for="orgSelect">Organization</label>
      <select id="orgSelect"></select>
      <div class="hint">If empty, save a valid key and click Refresh.</div>
    </div>
    </div>
    </div>

    <div id="codexSection" hidden>
    <div class="row">
      <label for="codexUsageSource">Codex usage source</label>
      <select id="codexUsageSource">
        <option value="oauth">OAuth (from codex auth.json)</option>
        <option value="cli">CLI (local codex)</option>
      </select>
      <div class="hint" id="codexHint"></div>
    </div>
    </div>

    <div class="row">
      <label for="refreshInterval">Refresh interval</label>
      <select id="refreshInterval">
        <option value="30">30 seconds</option>
        <option value="60">1 minute (default)</option>
        <option value="120">2 minutes</option>
        <option value="300">5 minutes</option>
        <option value="600">10 minutes</option>
      </select>
      <div class="hint">How often to check usage</div>
    </div>

    <div class="row">
      <div class="setting">
        <div class="setting-text">
          <label class="setting-title" for="notifyReset">Notify when usage resets</label>
          <div class="hint">Get notified when your session (5h) or weekly usage window resets</div>
        </div>
        <input id="notifyReset" class="setting-checkbox" type="checkbox" />
      </div>
    </div>

    <div class="row">
      <div class="setting">
        <div class="setting-text">
          <label class="setting-title" for="autostart">Start on login</label>
        </div>
        <input id="autostart" class="setting-checkbox" type="checkbox" />
      </div>
    </div>

    <div class="row">
      <div class="setting">
        <div class="setting-text">
          <label class="setting-title" for="updatesStartup">Check for updates on startup</label>
        </div>
        <input id="updatesStartup" class="setting-checkbox" type="checkbox" />
      </div>
    </div>

    <div class="buttons">
      <button id="refreshNow">Refresh now</button>
      <button id="forgetKey" class="danger" hidden>Forget</button>
      <button id="forgetClaudeKey" class="danger" hidden>Forget Claude key</button>
      <button id="save" class="primary">Save</button>
    </div>

    <div class="status" id="statusBox">Loading…</div>

    <div class="footer">
      <div class="footer-tagline">Free and open source ❤️</div>
      <div class="footer-links">
        <span class="footer-version">v1.3.0</span>
        <span class="footer-separator">•</span>
        <a href="#" id="githubLink" class="footer-link">GitHub</a>
        <span class="footer-separator">•</span>
        <a href="#" id="issuesLink" class="footer-link">Report Issue</a>
      </div>
    </div>
  `;

  const ui: Ui = {
    trackClaudeEl: el<HTMLInputElement>(root, '#trackClaude'),
    trackCodexEl: el<HTMLInputElement>(root, '#trackCodex'),
    claudeSectionEl: el<HTMLElement>(root, '#claudeSection'),
    codexSectionEl: el<HTMLElement>(root, '#codexSection'),

    usageSourceEl: el<HTMLSelectElement>(root, '#usageSource'),
    webOnlySectionEl: el<HTMLElement>(root, '#webOnlySection'),
    cliHintEl: el<HTMLElement>(root, '#cliHint'),
    sessionKeyEl: el<HTMLInputElement>(root, '#sessionKey'),
    rememberKeyEl: el<HTMLInputElement>(root, '#rememberKey'),
    claudeStorageHintEl: el<HTMLElement>(root, '#claudeStorageHint'),
    orgSelectEl: el<HTMLSelectElement>(root, '#orgSelect'),

    codexUsageSourceEl: el<HTMLSelectElement>(root, '#codexUsageSource'),
    codexHintEl: el<HTMLElement>(root, '#codexHint'),

    refreshIntervalEl: el<HTMLSelectElement>(root, '#refreshInterval'),
    notifyResetEl: el<HTMLInputElement>(root, '#notifyReset'),
    autostartEl: el<HTMLInputElement>(root, '#autostart'),
    updatesStartupEl: el<HTMLInputElement>(root, '#updatesStartup'),

    forgetKeyButton: el<HTMLButtonElement>(root, '#forgetKey'),
    forgetClaudeKeyButton: el<HTMLButtonElement>(root, '#forgetClaudeKey'),
    statusBoxEl: el<HTMLElement>(root, '#statusBox'),
  };

  const refreshNowButton = el<HTMLButtonElement>(root, '#refreshNow');
  const saveButton = el<HTMLButtonElement>(root, '#save');

  ui.trackClaudeEl.addEventListener('change', () => {
    applyVisibility(
      ui,
      ui.trackClaudeEl.checked,
      ui.trackCodexEl.checked,
      ui.usageSourceEl.value as UsageSource,
      ui.codexUsageSourceEl.value as CodexUsageSource,
    );
  });

  ui.trackCodexEl.addEventListener('change', () => {
    applyVisibility(
      ui,
      ui.trackClaudeEl.checked,
      ui.trackCodexEl.checked,
      ui.usageSourceEl.value as UsageSource,
      ui.codexUsageSourceEl.value as CodexUsageSource,
    );
  });
  ui.usageSourceEl.addEventListener('change', () => {
    applyVisibility(
      ui,
      ui.trackClaudeEl.checked,
      ui.trackCodexEl.checked,
      ui.usageSourceEl.value as UsageSource,
      ui.codexUsageSourceEl.value as CodexUsageSource,
    );
  });
  ui.codexUsageSourceEl.addEventListener('change', () => {
    applyVisibility(
      ui,
      ui.trackClaudeEl.checked,
      ui.trackCodexEl.checked,
      ui.usageSourceEl.value as UsageSource,
      ui.codexUsageSourceEl.value as CodexUsageSource,
    );
  });

  refreshNowButton.addEventListener('click', async () => {
    const result = await settingsRefreshNow();
    setResultError(ui.statusBoxEl, result);
    await loadState(ui);
  });

  ui.forgetKeyButton.addEventListener('click', async () => {
    const result = await settingsForgetKey();
    setResultError(ui.statusBoxEl, result);
    await loadState(ui);
  });

  ui.forgetClaudeKeyButton.addEventListener('click', async () => {
    const result = await settingsForgetClaudeKey();
    setResultError(ui.statusBoxEl, result);
    await loadState(ui);
  });

  saveButton.addEventListener('click', async () => {
    const usageSource = ui.usageSourceEl.value as UsageSource;
    const codexUsageSource = ui.codexUsageSourceEl.value as CodexUsageSource;
    const trackClaudeEnabled = ui.trackClaudeEl.checked;
    const trackCodexEnabled = ui.trackCodexEl.checked;

    const payload: SaveSettingsPayload = {
      trackClaudeEnabled,
      trackCodexEnabled,
      usageSource,
      sessionKey:
        trackClaudeEnabled && usageSource === 'web'
          ? ui.sessionKeyEl.value || undefined
          : undefined,
      rememberSessionKey: ui.rememberKeyEl.checked,
      codexUsageSource,
      refreshIntervalSeconds: Number(ui.refreshIntervalEl.value || 60),
      notifyOnUsageReset: ui.notifyResetEl.checked,
      autostartEnabled: ui.autostartEl.checked,
      checkUpdatesOnStartup: ui.updatesStartupEl.checked,
      selectedOrganizationId:
        trackClaudeEnabled && usageSource === 'web' ? ui.orgSelectEl.value || undefined : undefined,
    };

    const result = await settingsSave(payload);
    setResultError(ui.statusBoxEl, result);
    if (result.ok) {
      await loadState(ui);
    }
  });

  // Footer links
  const githubLink = el<HTMLAnchorElement>(root, '#githubLink');
  const issuesLink = el<HTMLAnchorElement>(root, '#issuesLink');

  githubLink.addEventListener('click', (e) => {
    e.preventDefault();
    void openUrl('https://github.com/leonardocouy/claudometer');
  });

  issuesLink.addEventListener('click', (e) => {
    e.preventDefault();
    void openUrl('https://github.com/leonardocouy/claudometer/issues');
  });

  void loadState(ui);

  void listen<UsageSnapshotBundle | null>('snapshot:updated', (event) => {
    setStatus(ui.statusBoxEl, renderSnapshot(event.payload));
  });

  const versionEl = root.querySelector('.footer-version');
  if (versionEl) {
    void getVersion()
      .then((version) => {
        versionEl.textContent = `v${version}`;
      })
      .catch(() => {});
  }
}

renderApp(el<HTMLElement>(document, '#app'));
