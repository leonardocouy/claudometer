/**
 * Claudometer - Electron Main Process (tray-first)
 */

import { app, Menu } from 'electron';
import type { ClaudeUsageSnapshot } from './core/types.ts';
import { nowIso } from './core/types.ts';
import { ClaudeApiService, type ClaudeOrganization } from './services/claude-api.ts';
import { SessionKeyService } from './services/session-key.ts';
import { SettingsService } from './services/settings.ts';
import {
  type SaveSettingsPayload,
  type SettingsState,
  SettingsWindowService,
} from './ui/settings-window/window.ts';
import { TrayService } from './ui/tray.ts';

const settingsService = new SettingsService();
const sessionKeyService = new SessionKeyService();
const claudeApiService = new ClaudeApiService();

let tray: TrayService | null = null;
let settingsWindow: SettingsWindowService | null = null;
let pollTimer: NodeJS.Timeout | null = null;

let organizations: ClaudeOrganization[] = [];
let latestSnapshot: ClaudeUsageSnapshot | null = null;

async function isKeytarAvailable(): Promise<boolean> {
  if (process.platform === 'linux') return false;
  try {
    await import('keytar');
    return true;
  } catch {
    return false;
  }
}

function stopPolling(): void {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

function startPolling(): void {
  stopPolling();
  const intervalSeconds = settingsService.getRefreshIntervalSeconds();
  pollTimer = setInterval(() => {
    void refreshAll();
  }, Math.max(10, intervalSeconds) * 1000);
}

function updateTray(snapshot: ClaudeUsageSnapshot | null): void {
  latestSnapshot = snapshot;
  tray?.updateSnapshot(snapshot);
}

async function resolveOrganizationId(sessionKey: string): Promise<string | null> {
  organizations = await claudeApiService.fetchOrganizations(sessionKey);
  const stored = settingsService.getSelectedOrganizationId();
  if (stored && organizations.some((o) => o.id === stored)) return stored;
  const first = organizations[0]?.id ?? null;
  if (first) settingsService.setSelectedOrganizationId(first);
  return first;
}

async function refreshAll(): Promise<void> {
  const sessionKey = await sessionKeyService.getCurrentKey();
  if (!sessionKey) {
    updateTray(sessionKeyService.buildMissingKeySnapshot());
    stopPolling();
    return;
  }

  let orgId: string | null = null;
  try {
    orgId = await resolveOrganizationId(sessionKey);
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to fetch organizations';
    updateTray({ status: 'error', lastUpdatedAt: nowIso(), errorMessage: message });
    return;
  }

  if (!orgId) {
    updateTray({
      status: 'error',
      lastUpdatedAt: nowIso(),
      errorMessage: 'No organizations found for this account.',
    });
    return;
  }

  const snapshot = await claudeApiService.fetchUsageSnapshot(sessionKey, orgId);
  updateTray(snapshot);

  if (snapshot.status === 'unauthorized') {
    stopPolling();
  } else if (snapshot.status === 'rate_limited') {
    stopPolling();
    setTimeout(
      () => {
        startPolling();
        void refreshAll();
      },
      5 * 60 * 1000,
    );
  }
}

async function getSettingsState(): Promise<SettingsState> {
  return {
    rememberSessionKey: settingsService.getRememberSessionKey(),
    refreshIntervalSeconds: settingsService.getRefreshIntervalSeconds(),
    organizations,
    selectedOrganizationId: settingsService.getSelectedOrganizationId(),
    latestSnapshot,
    keytarAvailable: await isKeytarAvailable(),
  };
}

async function saveSettings(
  payload: SaveSettingsPayload,
): Promise<{ ok: true } | { ok: false; error: string }> {
  const refreshIntervalSeconds = Number(payload.refreshIntervalSeconds);
  if (!Number.isFinite(refreshIntervalSeconds) || refreshIntervalSeconds < 10) {
    return { ok: false, error: 'Refresh interval must be >= 10 seconds.' };
  }

  const candidateSessionKey = payload.sessionKey?.trim();
  if (candidateSessionKey) {
    try {
      // Validate key before persisting or replacing any previously working key.
      const fetchedOrgs = await claudeApiService.fetchOrganizations(candidateSessionKey);
      organizations = fetchedOrgs;
      if (fetchedOrgs.length === 0) {
        return { ok: false, error: 'No organizations found for this account.' };
      }

      // Resolve org selection deterministically for the new key.
      const chosenOrgId = payload.selectedOrganizationId?.trim()
        ? payload.selectedOrganizationId?.trim()
        : settingsService.getSelectedOrganizationId();
      const resolvedOrgId =
        chosenOrgId && fetchedOrgs.some((o) => o.id === chosenOrgId)
          ? chosenOrgId
          : fetchedOrgs[0]?.id;
      settingsService.setSelectedOrganizationId(resolvedOrgId);

      // Only after validation do we replace the active key.
      sessionKeyService.setInMemory(candidateSessionKey);
      if (payload.rememberSessionKey) {
        await sessionKeyService.rememberKey(candidateSessionKey);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to validate session key.';
      return { ok: false, error: message };
    }
  }

  settingsService.setRefreshIntervalSeconds(refreshIntervalSeconds);
  // If a new key was provided we already resolved org selection above; otherwise accept user selection.
  if (!candidateSessionKey) {
    settingsService.setSelectedOrganizationId(payload.selectedOrganizationId);
  }
  settingsService.setRememberSessionKey(payload.rememberSessionKey);

  startPolling();
  await refreshAll();
  return { ok: true };
}

async function forgetKey(): Promise<void> {
  await sessionKeyService.forgetKey();
  updateTray(sessionKeyService.buildMissingKeySnapshot());
  stopPolling();
}

function openSettings(): void {
  settingsWindow ??= new SettingsWindowService({
    getState: getSettingsState,
    onSave: saveSettings,
    onForgetKey: forgetKey,
    onRefreshNow: refreshAll,
  });
  settingsWindow.show();
}

async function initialize(): Promise<void> {
  if (process.platform !== 'darwin') {
    Menu.setApplicationMenu(null);
  }

  if (process.platform === 'darwin') {
    app.dock?.hide();
  }

  tray = new TrayService({
    onOpenSettings: openSettings,
    onRefreshNow: () => void refreshAll(),
    onQuit: () => app.quit(),
  });

  await refreshAll();
  startPolling();
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
  stopPolling();
  tray?.destroy();
  tray = null;
});
