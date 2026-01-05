import './styles.css';
import type { ClaudeUsageSnapshot, ClaudeOrganization } from '../../shared/types.ts';
import type { IpcResult, SettingsState } from '../../shared/ipc.ts';

const el = <T extends HTMLElement>(root: ParentNode, selector: string): T => {
  const node = root.querySelector(selector);
  if (!node) throw new Error(`Missing element: ${selector}`);
  return node as T;
};

function renderOrgs(orgSelectEl: HTMLSelectElement, orgs: ClaudeOrganization[], selectedId?: string) {
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

async function loadState(
  sessionKeyEl: HTMLInputElement,
  rememberKeyEl: HTMLSelectElement,
  refreshIntervalEl: HTMLInputElement,
  orgSelectEl: HTMLSelectElement,
  statusBoxEl: HTMLElement,
  keytarHintEl: HTMLElement,
): Promise<SettingsState> {
  const state = await window.api.settings.getState();
  rememberKeyEl.value = String(Boolean(state.rememberSessionKey));
  refreshIntervalEl.value = String(state.refreshIntervalSeconds || 60);
  renderOrgs(orgSelectEl, state.organizations || [], state.selectedOrganizationId);
  keytarHintEl.textContent = state.keytarAvailable
    ? ''
    : 'On Linux, "Remember" saves to ~/.claudometer/session-key (chmod 600).';
  setStatus(statusBoxEl, renderSnapshot(state.latestSnapshot));
  sessionKeyEl.value = '';
  return state;
}

function renderApp(root: HTMLElement): void {
  root.innerHTML = `
    <h1>Claudometer</h1>

    <div class="row">
      <label for="sessionKey">Claude session key (from claude.ai cookies)</label>
      <input id="sessionKey" type="password" placeholder="sk-ant-sid01-..." autocomplete="off" />
      <div class="hint">Never paste this anywhere else. It is stored only if "Remember" is enabled.</div>
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
  `;

  const sessionKeyEl = el<HTMLInputElement>(root, '#sessionKey');
  const rememberKeyEl = el<HTMLSelectElement>(root, '#rememberKey');
  const refreshIntervalEl = el<HTMLInputElement>(root, '#refreshInterval');
  const orgSelectEl = el<HTMLSelectElement>(root, '#orgSelect');
  const statusBoxEl = el<HTMLElement>(root, '#statusBox');
  const keytarHintEl = el<HTMLElement>(root, '#keytarHint');

  const refreshNowButton = el<HTMLButtonElement>(root, '#refreshNow');
  const forgetKeyButton = el<HTMLButtonElement>(root, '#forgetKey');
  const saveButton = el<HTMLButtonElement>(root, '#save');

  refreshNowButton.addEventListener('click', async () => {
    const result = await window.api.settings.refreshNow();
    setResultError(statusBoxEl, result);
    await loadState(
      sessionKeyEl,
      rememberKeyEl,
      refreshIntervalEl,
      orgSelectEl,
      statusBoxEl,
      keytarHintEl,
    );
  });

  forgetKeyButton.addEventListener('click', async () => {
    const result = await window.api.settings.forgetKey();
    setResultError(statusBoxEl, result);
    await loadState(
      sessionKeyEl,
      rememberKeyEl,
      refreshIntervalEl,
      orgSelectEl,
      statusBoxEl,
      keytarHintEl,
    );
  });

  saveButton.addEventListener('click', async () => {
    const payload = {
      sessionKey: sessionKeyEl.value,
      rememberSessionKey: rememberKeyEl.value === 'true',
      refreshIntervalSeconds: Number(refreshIntervalEl.value || 60),
      selectedOrganizationId: orgSelectEl.value || undefined,
    };
    const result = await window.api.settings.save(payload);
    setResultError(statusBoxEl, result);
    if (result.ok) {
      await loadState(
        sessionKeyEl,
        rememberKeyEl,
        refreshIntervalEl,
        orgSelectEl,
        statusBoxEl,
        keytarHintEl,
      );
    }
  });

  void loadState(
    sessionKeyEl,
    rememberKeyEl,
    refreshIntervalEl,
    orgSelectEl,
    statusBoxEl,
    keytarHintEl,
  );

  window.api.settings.onSnapshotUpdated((snapshot) => {
    setStatus(statusBoxEl, renderSnapshot(snapshot));
  });
}

renderApp(el<HTMLElement>(document, '#app'));

