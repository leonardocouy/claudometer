import './styles.css';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { IpcResult, SettingsState } from '../../common/ipc.ts';
import type { ClaudeOrganization, ClaudeUsageSnapshot, UsageSource } from '../../common/types.ts';

const el = <T extends HTMLElement>(root: ParentNode, selector: string): T => {
  const node = root.querySelector(selector);
  if (!node) throw new Error(`Missing element: ${selector}`);
  return node as T;
};

async function settingsGetState(): Promise<SettingsState> {
  return await invoke<SettingsState>('settings_get_state');
}

async function settingsSave(payload: {
  sessionKey?: string;
  rememberSessionKey: boolean;
  refreshIntervalSeconds: number;
  notifyOnUsageReset: boolean;
  autostartEnabled: boolean;
  checkUpdatesOnStartup: boolean;
  selectedOrganizationId?: string;
}): Promise<IpcResult<null>> {
  return await invoke<IpcResult<null>>('settings_save', { payload });
}

async function settingsForgetKey(): Promise<IpcResult<null>> {
  return await invoke<IpcResult<null>>('settings_forget_key');
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

function renderSnapshotToElement(
  statusBoxEl: HTMLElement,
  snapshot: ClaudeUsageSnapshot | null,
): void {
  // Clear previous content
  statusBoxEl.textContent = '';

  // Create status label
  const statusLabel = document.createElement('strong');
  statusLabel.textContent = 'Status: ';
  statusBoxEl.appendChild(statusLabel);

  if (!snapshot) {
    statusBoxEl.appendChild(document.createTextNode('no data'));
    return;
  }

  if (snapshot.status !== 'ok') {
    // Status value
    const statusSpan = document.createElement('span');
    if (snapshot.status === 'error' || snapshot.status === 'unauthorized') {
      statusSpan.className = 'error';
    }
    statusSpan.textContent = snapshot.status;
    statusBoxEl.appendChild(statusSpan);

    // Error message if present
    if (snapshot.errorMessage) {
      statusBoxEl.appendChild(document.createElement('br'));
      const errorDiv = document.createElement('div');
      errorDiv.className = 'error';
      errorDiv.textContent = snapshot.errorMessage; // textContent prevents XSS
      statusBoxEl.appendChild(errorDiv);
    }

    // Last updated
    statusBoxEl.appendChild(document.createElement('br'));
    const lastUpdated = document.createElement('div');
    lastUpdated.textContent = `Last updated: ${snapshot.lastUpdatedAt}`;
    statusBoxEl.appendChild(lastUpdated);
    return;
  }

  // OK status - render usage data
  statusBoxEl.appendChild(document.createTextNode('ok'));
  statusBoxEl.appendChild(document.createElement('br'));

  statusBoxEl.appendChild(
    document.createTextNode(`Session: ${Math.round(snapshot.sessionPercent)}%`),
  );
  statusBoxEl.appendChild(document.createElement('br'));

  statusBoxEl.appendChild(
    document.createTextNode(`Weekly: ${Math.round(snapshot.weeklyPercent)}%`),
  );
  statusBoxEl.appendChild(document.createElement('br'));

  // Display first model from array (settings UI shows single model for simplicity)
  const firstModel = snapshot.models[0];
  if (firstModel) {
    const modelName = firstModel.name;
    const modelPercent = Math.round(firstModel.percent);
    let modelText = `${modelName} (weekly): ${modelPercent}%`;
    if (firstModel.resetsAt) {
      modelText += ` (resets ${new Date(firstModel.resetsAt).toLocaleString()})`;
    }
    statusBoxEl.appendChild(document.createTextNode(modelText));
    statusBoxEl.appendChild(document.createElement('br'));
  } else {
    statusBoxEl.appendChild(document.createTextNode('Model (weekly): --%'));
    statusBoxEl.appendChild(document.createElement('br'));
  }

  statusBoxEl.appendChild(document.createTextNode(`Last updated: ${snapshot.lastUpdatedAt}`));
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

async function loadState(
  sessionKeyEl: HTMLInputElement,
  rememberKeyEl: HTMLSelectElement,
  refreshIntervalEl: HTMLInputElement,
  notifyResetEl: HTMLSelectElement,
  autostartEl: HTMLSelectElement,
  updatesStartupEl: HTMLSelectElement,
  orgSelectEl: HTMLSelectElement,
  statusBoxEl: HTMLElement,
  storageHintEl: HTMLElement,
): Promise<SettingsState> {
  const state = await settingsGetState();
  rememberKeyEl.value = String(Boolean(state.rememberSessionKey));
  refreshIntervalEl.value = String(state.refreshIntervalSeconds || 60);
  notifyResetEl.value = String(state.notifyOnUsageReset ?? true);
  autostartEl.value = String(state.autostartEnabled ?? false);
  updatesStartupEl.value = String(state.checkUpdatesOnStartup ?? true);
  renderOrgs(orgSelectEl, state.organizations || [], state.selectedOrganizationId);
  rememberKeyEl.disabled = !state.keyringAvailable;
  if (!state.keyringAvailable) {
    rememberKeyEl.value = 'false';
  }
  storageHintEl.textContent = state.keyringAvailable
    ? ''
    : 'OS keychain/secret service is unavailable. “Remember session key” is disabled on this system.';
  setStatus(statusBoxEl, renderSnapshot(state.latestSnapshot));
  sessionKeyEl.value = '';
  return state;
}

function renderApp(root: HTMLElement): void {
  root.innerHTML = `
    <h1>Claudometer</h1>

    <div class="row">
      <label for="usageSource">Usage data source</label>
      <select id="usageSource">
        <option value="web">Claude Web (session key)</option>
        <option value="cli">Claude Code CLI</option>
      </select>
      <div class="hint">Choose how to fetch usage data</div>
    </div>

    <div id="webOnlySection">
      <div class="row">
        <label for="sessionKey">Claude session key (from claude.ai cookies)</label>
        <input id="sessionKey" type="password" placeholder="sk-ant-sid01-..." autocomplete="off" />
        <div class="hint">Never paste this anywhere else. It is stored only if "Remember" is enabled.</div>
      </div>

      <div class="row inline">
        <div>
          <label for="rememberKey">Remember session key (encrypted storage)</label>
          <select id="rememberKey">
            <option value="false">No (memory only)</option>
            <option value="true">Yes</option>
          </select>
          <div class="hint" id="storageHint"></div>
        </div>
        <div>
          <label for="orgSelect">Organization</label>
          <select id="orgSelect"></select>
          <div class="hint">If empty, save a valid key and click Refresh.</div>
        </div>
      </div>
    </div>

    <div id="cliOnlySection" style="display: none;">
      <div class="row">
        <div class="hint">Uses OAuth credentials from ~/.claude/.credentials.json automatically.<br/>Make sure you've authenticated with Claude Code CLI first.</div>
      </div>
    </div>

    <div class="row inline">
      <div>
        <label for="refreshInterval">Refresh interval (seconds)</label>
        <input id="refreshInterval" type="number" min="10" step="1" />
      </div>
    </div>

    <div class="row">
      <label for="notifyReset">Notify when usage periods reset</label>
      <select id="notifyReset">
        <option value="true">Yes (default)</option>
        <option value="false">No</option>
      </select>
      <div class="hint">Show notifications when 5-hour session or weekly usage windows reset</div>
    </div>

    <div class="row inline">
      <div>
        <label for="autostart">Start on login</label>
        <select id="autostart">
          <option value="false">No</option>
          <option value="true">Yes</option>
        </select>
      </div>
      <div>
        <label for="updatesStartup">Check for updates on startup</label>
        <select id="updatesStartup">
          <option value="true">Yes</option>
          <option value="false">No</option>
        </select>
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

    <div class="footer">
      <div class="footer-tagline">Free and open source</div>
      <div class="footer-links">
        <span class="footer-version">v1.3.0</span>
        <span class="footer-separator">•</span>
        <a href="#" id="githubLink" class="footer-link">GitHub</a>
        <span class="footer-separator">•</span>
        <a href="#" id="issuesLink" class="footer-link">Report Issue</a>
      </div>
    </div>
  `;

  const sessionKeyEl = el<HTMLInputElement>(root, '#sessionKey');
  const rememberKeyEl = el<HTMLSelectElement>(root, '#rememberKey');
  const refreshIntervalEl = el<HTMLInputElement>(root, '#refreshInterval');
  const notifyResetEl = el<HTMLSelectElement>(root, '#notifyReset');
  const autostartEl = el<HTMLSelectElement>(root, '#autostart');
  const updatesStartupEl = el<HTMLSelectElement>(root, '#updatesStartup');
  const orgSelectEl = el<HTMLSelectElement>(root, '#orgSelect');
  const statusBoxEl = el<HTMLElement>(root, '#statusBox');
  const storageHintEl = el<HTMLElement>(root, '#storageHint');

  const refreshNowButton = el<HTMLButtonElement>(root, '#refreshNow');
  const saveButton = el<HTMLButtonElement>(root, '#save');

  // Toggle visibility when source changes
  elements.usageSourceEl.addEventListener('change', () => {
    updateSourceVisibility(elements.usageSourceEl.value as UsageSource, elements);
  });

  refreshNowButton.addEventListener('click', async () => {
    const result = await settingsRefreshNow();
    setResultError(statusBoxEl, result);
    await loadState(
      sessionKeyEl,
      rememberKeyEl,
      refreshIntervalEl,
      notifyResetEl,
      autostartEl,
      updatesStartupEl,
      orgSelectEl,
      statusBoxEl,
      storageHintEl,
    );
  });

  forgetKeyButton.addEventListener('click', async () => {
    const result = await settingsForgetKey();
    setResultError(statusBoxEl, result);
    await loadState(
      sessionKeyEl,
      rememberKeyEl,
      refreshIntervalEl,
      notifyResetEl,
      autostartEl,
      updatesStartupEl,
      orgSelectEl,
      statusBoxEl,
      storageHintEl,
    );
  });

  saveButton.addEventListener('click', async () => {
    const usageSource = elements.usageSourceEl.value as UsageSource;
    const payload = {
      sessionKey: sessionKeyEl.value || undefined,
      rememberSessionKey: rememberKeyEl.value === 'true',
      refreshIntervalSeconds: Number(refreshIntervalEl.value || 60),
      notifyOnUsageReset: notifyResetEl.value === 'true',
      autostartEnabled: autostartEl.value === 'true',
      checkUpdatesOnStartup: updatesStartupEl.value === 'true',
      selectedOrganizationId: orgSelectEl.value || undefined,
    };
    const result = await settingsSave(payload);
    setResultError(statusBoxEl, result);
    if (result.ok) {
      await loadState(
        sessionKeyEl,
        rememberKeyEl,
        refreshIntervalEl,
        notifyResetEl,
        autostartEl,
        updatesStartupEl,
        orgSelectEl,
        statusBoxEl,
        storageHintEl,
      );
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

  void loadState(
    sessionKeyEl,
    rememberKeyEl,
    refreshIntervalEl,
    notifyResetEl,
    autostartEl,
    updatesStartupEl,
    orgSelectEl,
    statusBoxEl,
    storageHintEl,
  );

  void listen<ClaudeUsageSnapshot | null>('snapshot:updated', (event) => {
    setStatus(statusBoxEl, renderSnapshot(event.payload));
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
