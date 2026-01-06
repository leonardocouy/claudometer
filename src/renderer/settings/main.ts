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

  const modelName = snapshot.modelWeeklyName || 'Model';
  const modelPercent = Math.round(snapshot.modelWeeklyPercent);
  let modelText = `${modelName} (weekly): ${modelPercent}%`;
  if (snapshot.modelWeeklyResetsAt) {
    modelText += ` (resets ${new Date(snapshot.modelWeeklyResetsAt).toLocaleString()})`;
  }
  statusBoxEl.appendChild(document.createTextNode(modelText));
  statusBoxEl.appendChild(document.createElement('br'));

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
  elements.usageSourceEl.value = state.usageSource;
  elements.rememberKeyEl.value = String(Boolean(state.rememberSessionKey));
  elements.refreshIntervalEl.value = String(state.refreshIntervalSeconds || 60);
  elements.notifyResetEl.value = String(state.notifyOnUsageReset ?? true);
  renderOrgs(elements.orgSelectEl, state.organizations || [], state.selectedOrganizationId);
  elements.storageHintEl.textContent = state.encryptionAvailable
    ? ''
    : 'Encrypted storage is unavailable on this system. "Remember" will be memory-only (no persistence across restarts).';
  renderSnapshotToElement(elements.statusBoxEl, state.latestSnapshot);
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
    const payload = {
      sessionKey: elements.sessionKeyEl.value,
      rememberSessionKey: elements.rememberKeyEl.value === 'true',
      refreshIntervalSeconds: Number(elements.refreshIntervalEl.value || 60),
      notifyOnUsageReset: elements.notifyResetEl.value === 'true',
      selectedOrganizationId: elements.orgSelectEl.value || undefined,
      usageSource,
      claudeCliPath: 'claude', // Fixed value, not user-configurable
    };
    const result = await window.api.settings.save(payload);
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
    renderSnapshotToElement(elements.statusBoxEl, snapshot);
  });
}

renderApp(el<HTMLElement>(document, '#app'));
