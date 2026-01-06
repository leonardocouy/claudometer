import './styles.css';
import type { IpcResult, SettingsState } from '../../common/ipc.ts';
import type { ClaudeOrganization, ClaudeUsageSnapshot, UsageSource } from '../../common/types.ts';

const el = <T extends HTMLElement>(root: ParentNode, selector: string): T => {
  const node = root.querySelector(selector);
  if (!node) throw new Error(`Missing element: ${selector}`);
  return node as T;
};

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

function renderSnapshot(snapshot: ClaudeUsageSnapshot | null): string {
  if (!snapshot) return '<strong>Status:</strong> no data';
  if (snapshot.status !== 'ok') {
    const msg = snapshot.errorMessage ? `<div class="error">${snapshot.errorMessage}</div>` : '';
    return `<strong>Status:</strong> ${snapshot.status}${msg}<div>Last updated: ${snapshot.lastUpdatedAt}</div>`;
  }
  return `
    <strong>Status:</strong> ok<br/>
    Session: ${Math.round(snapshot.sessionPercent)}%<br/>
    Weekly: ${Math.round(snapshot.weeklyPercent)}%<br/>
    ${snapshot.modelWeeklyName || 'Model'} (weekly): ${Math.round(snapshot.modelWeeklyPercent)}%${
      snapshot.modelWeeklyResetsAt
        ? ` (resets ${new Date(snapshot.modelWeeklyResetsAt).toLocaleString()})`
        : ''
    }<br/>
    Last updated: ${snapshot.lastUpdatedAt}
  `;
}

function setStatus(statusBoxEl: HTMLElement, html: string): void {
  statusBoxEl.innerHTML = html;
}

function setResultError(statusBoxEl: HTMLElement, result: IpcResult<unknown>): void {
  if (result.ok) return;
  setStatus(
    statusBoxEl,
    `<strong>Status:</strong> <span class="error">error</span><div class="error">${result.error.message}</div>`,
  );
}

type Elements = {
  usageSourceEl: HTMLSelectElement;
  sessionKeyEl: HTMLInputElement;
  rememberKeyEl: HTMLSelectElement;
  refreshIntervalEl: HTMLInputElement;
  notifyResetEl: HTMLSelectElement;
  orgSelectEl: HTMLSelectElement;
  statusBoxEl: HTMLElement;
  storageHintEl: HTMLElement;
  webOnlySection: HTMLElement;
  cliOnlySection: HTMLElement;
  forgetKeyButton: HTMLButtonElement;
};

function updateSourceVisibility(source: UsageSource, elements: Elements): void {
  const isWebMode = source === 'web';
  elements.webOnlySection.style.display = isWebMode ? '' : 'none';
  elements.cliOnlySection.style.display = isWebMode ? 'none' : '';
  elements.forgetKeyButton.style.display = isWebMode ? '' : 'none';
}

async function loadState(elements: Elements): Promise<SettingsState> {
  const state = await window.api.settings.getState();
  console.log('[loadState] Received state:', { usageSource: state.usageSource });
  elements.usageSourceEl.value = state.usageSource;
  console.log('[loadState] Set select value to:', elements.usageSourceEl.value);
  elements.rememberKeyEl.value = String(Boolean(state.rememberSessionKey));
  elements.refreshIntervalEl.value = String(state.refreshIntervalSeconds || 60);
  elements.notifyResetEl.value = String(state.notifyOnUsageReset ?? true);
  renderOrgs(elements.orgSelectEl, state.organizations || [], state.selectedOrganizationId);
  elements.storageHintEl.textContent = state.encryptionAvailable
    ? ''
    : 'Encrypted storage is unavailable on this system. "Remember" will be memory-only (no persistence across restarts).';
  setStatus(elements.statusBoxEl, renderSnapshot(state.latestSnapshot));
  elements.sessionKeyEl.value = '';
  updateSourceVisibility(state.usageSource, elements);
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
      <div>
        <label for="notifyReset">Notify when usage periods reset</label>
        <select id="notifyReset">
          <option value="true">Yes (default)</option>
          <option value="false">No</option>
        </select>
      </div>
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

  const elements: Elements = {
    usageSourceEl: el<HTMLSelectElement>(root, '#usageSource'),
    sessionKeyEl: el<HTMLInputElement>(root, '#sessionKey'),
    rememberKeyEl: el<HTMLSelectElement>(root, '#rememberKey'),
    refreshIntervalEl: el<HTMLInputElement>(root, '#refreshInterval'),
    notifyResetEl: el<HTMLSelectElement>(root, '#notifyReset'),
    orgSelectEl: el<HTMLSelectElement>(root, '#orgSelect'),
    statusBoxEl: el<HTMLElement>(root, '#statusBox'),
    storageHintEl: el<HTMLElement>(root, '#storageHint'),
    webOnlySection: el<HTMLElement>(root, '#webOnlySection'),
    cliOnlySection: el<HTMLElement>(root, '#cliOnlySection'),
    forgetKeyButton: el<HTMLButtonElement>(root, '#forgetKey'),
  };

  const refreshNowButton = el<HTMLButtonElement>(root, '#refreshNow');
  const saveButton = el<HTMLButtonElement>(root, '#save');

  // Toggle visibility when source changes
  elements.usageSourceEl.addEventListener('change', () => {
    updateSourceVisibility(elements.usageSourceEl.value as UsageSource, elements);
  });

  refreshNowButton.addEventListener('click', async () => {
    const result = await window.api.settings.refreshNow();
    setResultError(elements.statusBoxEl, result);
    await loadState(elements);
  });

  elements.forgetKeyButton.addEventListener('click', async () => {
    const result = await window.api.settings.forgetKey();
    setResultError(elements.statusBoxEl, result);
    await loadState(elements);
  });

  saveButton.addEventListener('click', async () => {
    const usageSource = elements.usageSourceEl.value as UsageSource;
    console.log('[saveButton] Current select value:', usageSource);
    const payload = {
      sessionKey: elements.sessionKeyEl.value,
      rememberSessionKey: elements.rememberKeyEl.value === 'true',
      refreshIntervalSeconds: Number(elements.refreshIntervalEl.value || 60),
      notifyOnUsageReset: elements.notifyResetEl.value === 'true',
      selectedOrganizationId: elements.orgSelectEl.value || undefined,
      usageSource,
      claudeCliPath: 'claude', // Fixed value, not user-configurable
    };
    console.log('[saveButton] Sending payload:', { usageSource: payload.usageSource });
    const result = await window.api.settings.save(payload);
    console.log('[saveButton] Save result:', result);
    setResultError(elements.statusBoxEl, result);
    if (result.ok) {
      await loadState(elements);
    }
  });

  // Footer links
  const githubLink = el<HTMLAnchorElement>(root, '#githubLink');
  const issuesLink = el<HTMLAnchorElement>(root, '#issuesLink');

  githubLink.addEventListener('click', (e) => {
    e.preventDefault();
    window.open('https://github.com/leonardocouy/claudometer', '_blank');
  });

  issuesLink.addEventListener('click', (e) => {
    e.preventDefault();
    window.open('https://github.com/leonardocouy/claudometer/issues', '_blank');
  });

  void loadState(elements);

  window.api.settings.onSnapshotUpdated((snapshot) => {
    setStatus(elements.statusBoxEl, renderSnapshot(snapshot));
  });
}

renderApp(el<HTMLElement>(document, '#app'));
