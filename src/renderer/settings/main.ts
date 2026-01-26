import './styles.css';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import type {
  ClaudeOrganization,
  CodexUsageSource,
  IpcResult,
  SaveSettingsPayload,
  SettingsState,
  UsageSnapshotBundle,
  UsageSource,
} from '../../common/generated/ipc-types.ts';

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
  selectedId: string | null,
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

function getProgressBarClass(percent: number): string {
  if (percent > 80) return 'progress-bar-fill progress-red';
  if (percent > 50) return 'progress-bar-fill progress-yellow';
  return 'progress-bar-fill progress-green';
}

function setCardExpanded(card: HTMLElement, content: HTMLElement, expanded: boolean): void {
  card.dataset.expanded = String(expanded);
  content.toggleAttribute('hidden', !expanded);
}

function setupCardInteraction(
  card: HTMLElement,
  content: HTMLElement,
  toggle: HTMLInputElement,
  onToggleChange: () => void,
): void {
  const header = card.querySelector('.card-header');
  if (!header) return;

  // Click on header (excluding toggle) toggles the tracking on/off
  header.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;
    // Don't trigger if clicking the toggle switch or its label
    if (target.closest('.toggle-switch')) return;

    // Toggle the checkbox - this triggers expand/collapse via change event
    toggle.checked = !toggle.checked;
    toggle.dispatchEvent(new Event('change', { bubbles: true }));
  });

  // Toggle change controls expand/collapse and enables/disables tracking
  toggle.addEventListener('change', () => {
    setCardExpanded(card, content, toggle.checked);
    onToggleChange();
  });

  // Keyboard accessibility for card header
  header.setAttribute('tabindex', '0');
  header.setAttribute('role', 'button');
  header.setAttribute('aria-expanded', card.dataset.expanded || 'false');
  header.setAttribute(
    'aria-label',
    `Toggle ${card.querySelector('.card-title')?.textContent || 'provider'} tracking`,
  );

  header.addEventListener('keydown', ((e: KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      // Toggle the checkbox when pressing enter/space on header
      toggle.checked = !toggle.checked;
      toggle.dispatchEvent(new Event('change', { bubbles: true }));
    }
  }) as EventListener);

  // Update aria-expanded when card state changes
  const observer = new MutationObserver(() => {
    header.setAttribute('aria-expanded', card.dataset.expanded || 'false');
  });
  observer.observe(card, { attributes: true, attributeFilter: ['data-expanded'] });
}

function updateUsageStats(ui: Ui, snapshot: UsageSnapshotBundle | null): void {
  // Update Claude stats
  if (snapshot?.claude) {
    const claude = snapshot.claude;
    if (claude.status === 'ok') {
      const sessionPct = Math.round(claude.sessionPercent);
      const weeklyPct = Math.round(claude.weeklyPercent);

      ui.claudeSessionValueEl.textContent = `${sessionPct}%`;
      ui.claudeSessionBarEl.style.width = `${sessionPct}%`;
      ui.claudeSessionBarEl.className = getProgressBarClass(sessionPct);

      ui.claudeWeeklyValueEl.textContent = `${weeklyPct}%`;
      ui.claudeWeeklyBarEl.style.width = `${weeklyPct}%`;
      ui.claudeWeeklyBarEl.className = getProgressBarClass(weeklyPct);

      ui.claudeLastUpdatedEl.textContent = `Last updated: ${claude.lastUpdatedAt}`;
    } else {
      ui.claudeSessionValueEl.textContent = '--';
      ui.claudeSessionBarEl.style.width = '0%';
      ui.claudeWeeklyValueEl.textContent = '--';
      ui.claudeWeeklyBarEl.style.width = '0%';
      ui.claudeLastUpdatedEl.textContent = claude.errorMessage
        ? `Error: ${claude.errorMessage}`
        : `Status: ${claude.status}`;
    }
  } else {
    ui.claudeSessionValueEl.textContent = '--%';
    ui.claudeSessionBarEl.style.width = '0%';
    ui.claudeWeeklyValueEl.textContent = '--%';
    ui.claudeWeeklyBarEl.style.width = '0%';
    ui.claudeLastUpdatedEl.textContent = 'Last updated: --';
  }

  // Update Codex stats
  if (snapshot?.codex) {
    const codex = snapshot.codex;
    if (codex.status === 'ok') {
      const sessionPct = Math.round(codex.sessionPercent);
      const weeklyPct = Math.round(codex.weeklyPercent);

      ui.codexSessionValueEl.textContent = `${sessionPct}%`;
      ui.codexSessionBarEl.style.width = `${sessionPct}%`;
      ui.codexSessionBarEl.className = getProgressBarClass(sessionPct);

      ui.codexWeeklyValueEl.textContent = `${weeklyPct}%`;
      ui.codexWeeklyBarEl.style.width = `${weeklyPct}%`;
      ui.codexWeeklyBarEl.className = getProgressBarClass(weeklyPct);

      ui.codexLastUpdatedEl.textContent = `Last updated: ${codex.lastUpdatedAt}`;
    } else {
      ui.codexSessionValueEl.textContent = '--';
      ui.codexSessionBarEl.style.width = '0%';
      ui.codexWeeklyValueEl.textContent = '--';
      ui.codexWeeklyBarEl.style.width = '0%';
      ui.codexLastUpdatedEl.textContent = codex.errorMessage
        ? `Error: ${codex.errorMessage}`
        : `Status: ${codex.status}`;
    }
  } else {
    ui.codexSessionValueEl.textContent = '--%';
    ui.codexSessionBarEl.style.width = '0%';
    ui.codexWeeklyValueEl.textContent = '--%';
    ui.codexWeeklyBarEl.style.width = '0%';
    ui.codexLastUpdatedEl.textContent = 'Last updated: --';
  }
}

function renderSnapshot(snapshot: UsageSnapshotBundle | null): string {
  if (!snapshot) return '';

  const errors: string[] = [];

  // Only show errors, not OK status (it's redundant with provider cards)
  if (snapshot.claude && snapshot.claude.status !== 'ok') {
    if (snapshot.claude.errorMessage) {
      errors.push(
        `<strong>Claude:</strong> <span class="error">${escapeHtml(snapshot.claude.errorMessage)}</span>`,
      );
    } else {
      errors.push(`<strong>Claude:</strong> ${snapshot.claude.status}`);
    }
  }

  if (snapshot.codex && snapshot.codex.status !== 'ok') {
    if (snapshot.codex.errorMessage) {
      errors.push(
        `<strong>Codex:</strong> <span class="error">${escapeHtml(snapshot.codex.errorMessage)}</span>`,
      );
    } else {
      errors.push(`<strong>Codex:</strong> ${snapshot.codex.status}`);
    }
  }

  return errors.join(' | ');
}

function setStatus(statusBoxEl: HTMLElement, html: string): void {
  statusBoxEl.innerHTML = html;
}

function setResultError(statusBoxEl: HTMLElement, result: IpcResult<unknown>): void {
  if (result.ok) return;
  if (!('error' in result)) return;

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
  claudeCardEl: HTMLElement;
  codexCardEl: HTMLElement;
  claudeSectionEl: HTMLElement;
  codexSectionEl: HTMLElement;

  // Claude usage stats
  claudeSessionValueEl: HTMLElement;
  claudeSessionBarEl: HTMLElement;
  claudeWeeklyValueEl: HTMLElement;
  claudeWeeklyBarEl: HTMLElement;
  claudeLastUpdatedEl: HTMLElement;

  // Codex usage stats
  codexSessionValueEl: HTMLElement;
  codexSessionBarEl: HTMLElement;
  codexWeeklyValueEl: HTMLElement;
  codexWeeklyBarEl: HTMLElement;
  codexLastUpdatedEl: HTMLElement;

  // Source labels (in cards)
  claudeSourceLabelEl: HTMLElement;
  claudeSourceHintEl: HTMLElement;
  codexSourceLabelEl: HTMLElement;
  codexSourceHintEl: HTMLElement;

  // Modal elements
  modalBackdropEl: HTMLElement;
  modalTitleEl: HTMLElement;
  claudeConfigContentEl: HTMLElement;
  codexConfigContentEl: HTMLElement;

  // Claude config (in modal)
  usageSourceEl: HTMLSelectElement;
  webOnlySectionEl: HTMLElement;
  sessionKeyEl: HTMLInputElement;
  rememberKeyEl: HTMLInputElement;
  claudeStorageHintEl: HTMLElement;
  orgSelectEl: HTMLSelectElement;
  forgetClaudeKeyButton: HTMLButtonElement;

  // Codex config (in modal)
  codexUsageSourceEl: HTMLSelectElement;
  codexHintEl: HTMLElement;

  // Global settings
  refreshIntervalEl: HTMLSelectElement;
  notifyResetEl: HTMLInputElement;
  autostartEl: HTMLInputElement;
  updatesStartupEl: HTMLInputElement;

  forgetKeyButton: HTMLButtonElement;
  statusBoxEl: HTMLElement;
};

function applyVisibility(
  ui: Ui,
  trackClaudeEnabled: boolean,
  trackCodexEnabled: boolean,
  claudeSource: UsageSource,
  codexSource: CodexUsageSource,
) {
  // Update card expanded state and content visibility
  ui.claudeCardEl.dataset.expanded = String(trackClaudeEnabled);
  ui.codexCardEl.dataset.expanded = String(trackCodexEnabled);
  ui.claudeSectionEl.toggleAttribute('hidden', !trackClaudeEnabled);
  ui.codexSectionEl.toggleAttribute('hidden', !trackCodexEnabled);

  // Update Claude source label
  ui.claudeSourceLabelEl.textContent = claudeSource === 'web' ? 'Web (session key)' : 'Claude Code';
  ui.claudeSourceHintEl.textContent =
    claudeSource === 'web' ? 'Uses claude.ai cookie' : 'Uses Claude Code login';

  // Update Codex source label
  ui.codexSourceLabelEl.textContent = codexSource === 'cli' ? 'CLI' : 'OAuth';
  ui.codexSourceHintEl.textContent =
    codexSource === 'cli' ? 'Uses local codex CLI' : 'Uses ~/.codex/auth.json';

  // Modal: web-only section visibility
  ui.webOnlySectionEl.toggleAttribute('hidden', claudeSource !== 'web');
  ui.forgetClaudeKeyButton.toggleAttribute('hidden', claudeSource !== 'web');

  // Codex hint in modal
  ui.codexHintEl.textContent =
    codexSource === 'cli'
      ? 'Uses the local codex CLI (no network).'
      : 'Uses your local Codex login (reads ~/.codex/auth.json).';

  ui.forgetKeyButton.toggleAttribute('hidden', true);
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
    : 'OS keychain/secret service is unavailable. "Remember session key" is disabled on this system.';

  // Update usage stats in provider cards
  updateUsageStats(ui, state.latestSnapshot);
  setStatus(ui.statusBoxEl, renderSnapshot(state.latestSnapshot));
  ui.sessionKeyEl.value = '';
  return state;
}

function renderApp(root: HTMLElement): void {
  root.innerHTML = `
    <div class="settings-container">
      <div class="header">
        <h1>Claudometer</h1>
      </div>

      <!-- Providers Row (Side-by-side) -->
      <div class="providers-row">
        <!-- Claude Provider Card -->
        <div class="provider-card" id="claudeCard" data-expanded="false">
          <div class="card-header">
          <div class="card-header-info">
            <span class="card-title">Claude</span>
            <span class="card-subtitle">Anthropic usage tracking</span>
          </div>
          <label class="toggle-switch">
            <input type="checkbox" id="trackClaude" />
            <span class="slider"></span>
          </label>
        </div>
        <div class="card-content" id="claudeSection">
          <div class="usage-stats" id="claudeUsageStats">
            <div class="usage-stat">
              <div class="usage-stat-header">
                <span class="usage-stat-label">Session (5h)</span>
                <span class="usage-stat-value" id="claudeSessionValue">--%</span>
              </div>
              <div class="progress-bar">
                <div class="progress-bar-fill" id="claudeSessionBar" style="width: 0%"></div>
              </div>
            </div>
            <div class="usage-stat">
              <div class="usage-stat-header">
                <span class="usage-stat-label">Weekly</span>
                <span class="usage-stat-value" id="claudeWeeklyValue">--%</span>
              </div>
              <div class="progress-bar">
                <div class="progress-bar-fill" id="claudeWeeklyBar" style="width: 0%"></div>
              </div>
            </div>
            <div class="usage-stat-footer" id="claudeLastUpdated">Last updated: --</div>
          </div>

          <div class="source-row">
            <div class="source-info">
              <span class="source-label" id="claudeSourceLabel">Claude Code</span>
              <span class="source-hint" id="claudeSourceHint">Uses Claude Code login</span>
            </div>
            <button type="button" class="btn-configure" id="claudeConfigureBtn">Configure</button>
          </div>
        </div>
      </div>

      <!-- Codex Provider Card -->
      <div class="provider-card" id="codexCard" data-expanded="false">
        <div class="card-header">
          <div class="card-header-info">
            <span class="card-title">Codex</span>
            <span class="card-subtitle">OpenAI usage tracking</span>
          </div>
          <label class="toggle-switch">
            <input type="checkbox" id="trackCodex" />
            <span class="slider"></span>
          </label>
        </div>
        <div class="card-content" id="codexSection" hidden>
          <div class="usage-stats" id="codexUsageStats">
            <div class="usage-stat">
              <div class="usage-stat-header">
                <span class="usage-stat-label">Session</span>
                <span class="usage-stat-value" id="codexSessionValue">--%</span>
              </div>
              <div class="progress-bar">
                <div class="progress-bar-fill" id="codexSessionBar" style="width: 0%"></div>
              </div>
            </div>
            <div class="usage-stat">
              <div class="usage-stat-header">
                <span class="usage-stat-label">Weekly</span>
                <span class="usage-stat-value" id="codexWeeklyValue">--%</span>
              </div>
              <div class="progress-bar">
                <div class="progress-bar-fill" id="codexWeeklyBar" style="width: 0%"></div>
              </div>
            </div>
            <div class="usage-stat-footer" id="codexLastUpdated">Last updated: --</div>
          </div>

          <div class="source-row">
            <div class="source-info">
              <span class="source-label" id="codexSourceLabel">OAuth</span>
              <span class="source-hint" id="codexSourceHint">Uses local Codex login</span>
            </div>
            <button type="button" class="btn-configure" id="codexConfigureBtn">Configure</button>
          </div>
        </div>
      </div>
      </div><!-- End providers-row -->

      <!-- General Settings Card -->
      <div class="general-card">
        <div class="card-header">
          <span class="card-title">Settings</span>
        </div>
        <div class="card-content">
          <div class="settings-grid">
            <div class="setting">
              <div class="setting-text">
                <label class="setting-title" for="refreshInterval">Refresh interval</label>
              </div>
              <select id="refreshInterval" class="setting-select">
                <option value="30">30s</option>
                <option value="60">1m</option>
                <option value="120">2m</option>
                <option value="300">5m</option>
                <option value="600">10m</option>
              </select>
            </div>

            <div class="setting">
              <div class="setting-text">
                <label class="setting-title" for="notifyReset">Notify on reset</label>
              </div>
              <label class="toggle-switch toggle-switch-small">
                <input type="checkbox" id="notifyReset" />
                <span class="slider"></span>
              </label>
            </div>

            <div class="setting">
              <div class="setting-text">
                <label class="setting-title" for="autostart">Start on login</label>
              </div>
              <label class="toggle-switch toggle-switch-small">
                <input type="checkbox" id="autostart" />
                <span class="slider"></span>
              </label>
            </div>

            <div class="setting">
              <div class="setting-text">
                <label class="setting-title" for="updatesStartup">Check for updates</label>
              </div>
              <label class="toggle-switch toggle-switch-small">
                <input type="checkbox" id="updatesStartup" />
                <span class="slider"></span>
              </label>
            </div>
          </div>
        </div>
      </div>

      <!-- Actions Card -->
      <div class="actions-card">
        <div class="buttons">
          <button id="refreshNow">Refresh now</button>
          <button id="forgetKey" class="danger" hidden>Forget</button>
          <button id="save" class="primary">Save</button>
        </div>
      </div>

      <div class="status" id="statusBox"></div>

      <div class="footer">
        <div class="footer-tagline">Free and open source</div>
        <div class="footer-links">
          <span class="footer-version">v1.3.0</span>
          <span class="footer-separator">|</span>
          <a href="#" id="githubLink" class="footer-link">GitHub</a>
          <span class="footer-separator">|</span>
          <a href="#" id="issuesLink" class="footer-link">Report Issue</a>
        </div>
      </div>
    </div>

    <!-- Configuration Modal -->
    <div class="modal-backdrop" id="modalBackdrop" hidden>
      <div class="modal" id="configModal">
        <div class="modal-header">
          <span class="modal-title" id="modalTitle">Configuration</span>
          <button type="button" class="modal-close" id="modalClose">&times;</button>
        </div>
        <div class="modal-content" id="modalContent">
          <!-- Claude Config -->
          <div id="claudeConfigContent" hidden>
            <div class="row">
              <label for="usageSource">Usage data source</label>
              <select id="usageSource">
                <option value="cli">Claude Code</option>
                <option value="web">Claude Web (session key cookie)</option>
              </select>
            </div>

            <div id="webOnlySection">
              <div class="row">
                <label for="sessionKey">Session key (from claude.ai cookies)</label>
                <input id="sessionKey" type="password" placeholder="sk-ant-sid01-..." autocomplete="off" />
                <div class="hint">Never paste this anywhere else.</div>
              </div>

              <div class="row">
                <label for="orgSelect">Organization</label>
                <select id="orgSelect"></select>
              </div>

              <div class="row">
                <div class="setting">
                  <div class="setting-text">
                    <label class="setting-title" for="rememberKey">Remember session key</label>
                    <div class="hint" id="claudeStorageHint"></div>
                  </div>
                  <label class="toggle-switch toggle-switch-small">
                    <input type="checkbox" id="rememberKey" />
                    <span class="slider"></span>
                  </label>
                </div>
              </div>

              <button id="forgetClaudeKey" class="danger" type="button">Forget Claude key</button>
            </div>
          </div>

          <!-- Codex Config -->
          <div id="codexConfigContent" hidden>
            <div class="row">
              <label for="codexUsageSource">Usage data source</label>
              <select id="codexUsageSource">
                <option value="oauth">OAuth (from codex auth.json)</option>
                <option value="cli">CLI (local codex)</option>
              </select>
              <div class="hint" id="codexHint"></div>
            </div>
          </div>
        </div>
        <div class="modal-footer">
          <button type="button" class="modal-btn" id="modalCancel">Cancel</button>
          <button type="button" class="modal-btn primary" id="modalSave">Save</button>
        </div>
      </div>
    </div>
  `;

  const ui: Ui = {
    trackClaudeEl: el<HTMLInputElement>(root, '#trackClaude'),
    trackCodexEl: el<HTMLInputElement>(root, '#trackCodex'),
    claudeCardEl: el<HTMLElement>(root, '#claudeCard'),
    codexCardEl: el<HTMLElement>(root, '#codexCard'),
    claudeSectionEl: el<HTMLElement>(root, '#claudeSection'),
    codexSectionEl: el<HTMLElement>(root, '#codexSection'),

    // Claude usage stats
    claudeSessionValueEl: el<HTMLElement>(root, '#claudeSessionValue'),
    claudeSessionBarEl: el<HTMLElement>(root, '#claudeSessionBar'),
    claudeWeeklyValueEl: el<HTMLElement>(root, '#claudeWeeklyValue'),
    claudeWeeklyBarEl: el<HTMLElement>(root, '#claudeWeeklyBar'),
    claudeLastUpdatedEl: el<HTMLElement>(root, '#claudeLastUpdated'),

    // Codex usage stats
    codexSessionValueEl: el<HTMLElement>(root, '#codexSessionValue'),
    codexSessionBarEl: el<HTMLElement>(root, '#codexSessionBar'),
    codexWeeklyValueEl: el<HTMLElement>(root, '#codexWeeklyValue'),
    codexWeeklyBarEl: el<HTMLElement>(root, '#codexWeeklyBar'),
    codexLastUpdatedEl: el<HTMLElement>(root, '#codexLastUpdated'),

    // Source labels (in cards)
    claudeSourceLabelEl: el<HTMLElement>(root, '#claudeSourceLabel'),
    claudeSourceHintEl: el<HTMLElement>(root, '#claudeSourceHint'),
    codexSourceLabelEl: el<HTMLElement>(root, '#codexSourceLabel'),
    codexSourceHintEl: el<HTMLElement>(root, '#codexSourceHint'),

    // Modal elements
    modalBackdropEl: el<HTMLElement>(root, '#modalBackdrop'),
    modalTitleEl: el<HTMLElement>(root, '#modalTitle'),
    claudeConfigContentEl: el<HTMLElement>(root, '#claudeConfigContent'),
    codexConfigContentEl: el<HTMLElement>(root, '#codexConfigContent'),

    // Claude config (in modal)
    usageSourceEl: el<HTMLSelectElement>(root, '#usageSource'),
    webOnlySectionEl: el<HTMLElement>(root, '#webOnlySection'),
    sessionKeyEl: el<HTMLInputElement>(root, '#sessionKey'),
    rememberKeyEl: el<HTMLInputElement>(root, '#rememberKey'),
    claudeStorageHintEl: el<HTMLElement>(root, '#claudeStorageHint'),
    orgSelectEl: el<HTMLSelectElement>(root, '#orgSelect'),
    forgetClaudeKeyButton: el<HTMLButtonElement>(root, '#forgetClaudeKey'),

    // Codex config (in modal)
    codexUsageSourceEl: el<HTMLSelectElement>(root, '#codexUsageSource'),
    codexHintEl: el<HTMLElement>(root, '#codexHint'),

    // Global settings
    refreshIntervalEl: el<HTMLSelectElement>(root, '#refreshInterval'),
    notifyResetEl: el<HTMLInputElement>(root, '#notifyReset'),
    autostartEl: el<HTMLInputElement>(root, '#autostart'),
    updatesStartupEl: el<HTMLInputElement>(root, '#updatesStartup'),

    forgetKeyButton: el<HTMLButtonElement>(root, '#forgetKey'),
    statusBoxEl: el<HTMLElement>(root, '#statusBox'),
  };

  const refreshNowButton = el<HTMLButtonElement>(root, '#refreshNow');
  const saveButton = el<HTMLButtonElement>(root, '#save');

  // Helper to call applyVisibility with current state
  const updateVisibility = () => {
    applyVisibility(
      ui,
      ui.trackClaudeEl.checked,
      ui.trackCodexEl.checked,
      ui.usageSourceEl.value as UsageSource,
      ui.codexUsageSourceEl.value as CodexUsageSource,
    );
  };

  // Set up Claude card interaction (header click, toggle, keyboard)
  setupCardInteraction(ui.claudeCardEl, ui.claudeSectionEl, ui.trackClaudeEl, updateVisibility);

  // Set up Codex card interaction (header click, toggle, keyboard)
  setupCardInteraction(ui.codexCardEl, ui.codexSectionEl, ui.trackCodexEl, updateVisibility);

  // Modal controls
  const claudeConfigureBtn = el<HTMLButtonElement>(root, '#claudeConfigureBtn');
  const codexConfigureBtn = el<HTMLButtonElement>(root, '#codexConfigureBtn');
  const modalCloseBtn = el<HTMLButtonElement>(root, '#modalClose');
  const modalCancelBtn = el<HTMLButtonElement>(root, '#modalCancel');
  const modalSaveBtn = el<HTMLButtonElement>(root, '#modalSave');

  const openModal = (provider: 'claude' | 'codex') => {
    ui.modalTitleEl.textContent = provider === 'claude' ? 'Claude Configuration' : 'Codex Configuration';
    ui.claudeConfigContentEl.toggleAttribute('hidden', provider !== 'claude');
    ui.codexConfigContentEl.toggleAttribute('hidden', provider !== 'codex');
    ui.modalBackdropEl.removeAttribute('hidden');
  };

  const closeModal = () => {
    ui.modalBackdropEl.setAttribute('hidden', '');
  };

  claudeConfigureBtn.addEventListener('click', () => openModal('claude'));
  codexConfigureBtn.addEventListener('click', () => openModal('codex'));
  modalCloseBtn.addEventListener('click', closeModal);
  modalCancelBtn.addEventListener('click', closeModal);
  ui.modalBackdropEl.addEventListener('click', (e) => {
    if (e.target === ui.modalBackdropEl) closeModal();
  });

  modalSaveBtn.addEventListener('click', async () => {
    // Save the configuration from modal
    const usageSource = ui.usageSourceEl.value as UsageSource;
    const codexUsageSource = ui.codexUsageSourceEl.value as CodexUsageSource;
    const trackClaudeEnabled = ui.trackClaudeEl.checked;
    const trackCodexEnabled = ui.trackCodexEl.checked;

    const sessionKey =
      trackClaudeEnabled && usageSource === 'web' ? ui.sessionKeyEl.value.trim() : '';
    const selectedOrganizationId =
      trackClaudeEnabled && usageSource === 'web' ? ui.orgSelectEl.value.trim() : '';

    const payload: SaveSettingsPayload = {
      trackClaudeEnabled,
      trackCodexEnabled,
      usageSource,
      sessionKey: sessionKey ? sessionKey : null,
      rememberSessionKey: ui.rememberKeyEl.checked,
      codexUsageSource,
      refreshIntervalSeconds: Number(ui.refreshIntervalEl.value || 60),
      notifyOnUsageReset: ui.notifyResetEl.checked,
      autostartEnabled: ui.autostartEl.checked,
      checkUpdatesOnStartup: ui.updatesStartupEl.checked,
      selectedOrganizationId: selectedOrganizationId ? selectedOrganizationId : null,
    };

    const result = await settingsSave(payload);
    setResultError(ui.statusBoxEl, result);
    if (result.ok) {
      await loadState(ui);
      closeModal();
    }
  });

  // Source dropdown changes in modal
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

    const sessionKey =
      trackClaudeEnabled && usageSource === 'web' ? ui.sessionKeyEl.value.trim() : '';
    const selectedOrganizationId =
      trackClaudeEnabled && usageSource === 'web' ? ui.orgSelectEl.value.trim() : '';

    const payload: SaveSettingsPayload = {
      trackClaudeEnabled,
      trackCodexEnabled,
      usageSource,
      sessionKey: sessionKey ? sessionKey : null,
      rememberSessionKey: ui.rememberKeyEl.checked,
      codexUsageSource,
      refreshIntervalSeconds: Number(ui.refreshIntervalEl.value || 60),
      notifyOnUsageReset: ui.notifyResetEl.checked,
      autostartEnabled: ui.autostartEl.checked,
      checkUpdatesOnStartup: ui.updatesStartupEl.checked,
      selectedOrganizationId: selectedOrganizationId ? selectedOrganizationId : null,
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
    updateUsageStats(ui, event.payload);
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
