import type { IpcResult, SaveSettingsPayload, SettingsState } from '../common/ipc.ts';
import { type ClaudeOrganization, type ClaudeUsageSnapshot, nowIso } from '../common/types.ts';
import { type ClaudeApiService, getClaudeWebRequestErrorStatus } from './services/claudeApi.ts';
import type { SessionKeyService } from './services/sessionKey.ts';
import type { SettingsService } from './services/settings.ts';
import type { UsageNotificationService } from './services/usageNotification.ts';
import type { TrayService } from './tray.ts';

function sanitizeMessage(message: string): string {
  let sanitized = message.replaceAll(/sessionKey=[^;\s]+/gi, 'sessionKey=REDACTED');
  sanitized = sanitized.replaceAll(/sk-ant-sid01-[A-Za-z0-9_-]+/g, 'sk-ant-sid01-REDACTED');
  return sanitized;
}

function withJitter(baseMs: number, jitterFraction = 0.2): number {
  const jitter = baseMs * jitterFraction;
  const delta = (Math.random() * 2 - 1) * jitter;
  return Math.max(0, Math.round(baseMs + delta));
}

export class AppController {
  private settingsService: SettingsService;
  private sessionKeyService: SessionKeyService;
  private claudeApiService: ClaudeApiService;
  private trayService: TrayService;
  private usageNotificationService: UsageNotificationService;

  private organizations: ClaudeOrganization[] = [];
  private latestSnapshot: ClaudeUsageSnapshot | null = null;

  private running = false;
  private timer: NodeJS.Timeout | null = null;
  private currentRun: Promise<void> | null = null;
  private pendingImmediate = false;

  private snapshotListeners = new Set<(snapshot: ClaudeUsageSnapshot | null) => void>();

  constructor(options: {
    settingsService: SettingsService;
    sessionKeyService: SessionKeyService;
    claudeApiService: ClaudeApiService;
    trayService: TrayService;
    usageNotificationService: UsageNotificationService;
  }) {
    this.settingsService = options.settingsService;
    this.sessionKeyService = options.sessionKeyService;
    this.claudeApiService = options.claudeApiService;
    this.trayService = options.trayService;
    this.usageNotificationService = options.usageNotificationService;
  }

  onSnapshotUpdated(listener: (snapshot: ClaudeUsageSnapshot | null) => void): () => void {
    this.snapshotListeners.add(listener);
    return () => this.snapshotListeners.delete(listener);
  }

  start(): void {
    if (this.running) return;
    this.running = true;
    this.scheduleNext(0);
  }

  stop(): void {
    this.running = false;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  async getState(): Promise<SettingsState> {
    return {
      rememberSessionKey: this.settingsService.getRememberSessionKey(),
      refreshIntervalSeconds: this.settingsService.getRefreshIntervalSeconds(),
      notifyOnUsageReset: this.settingsService.getNotifyOnUsageReset(),
      organizations: this.organizations,
      selectedOrganizationId: this.settingsService.getSelectedOrganizationId(),
      latestSnapshot: this.latestSnapshot,
      encryptionAvailable: this.sessionKeyService.isEncryptionAvailable(),
    };
  }

  async saveSettings(payload: unknown): Promise<IpcResult<null>> {
    const parsed = payload as SaveSettingsPayload;
    const refreshIntervalSeconds = Number(parsed.refreshIntervalSeconds);
    if (!Number.isFinite(refreshIntervalSeconds) || refreshIntervalSeconds < 10) {
      return {
        ok: false,
        error: { code: 'VALIDATION', message: 'Refresh interval must be >= 10 seconds.' },
      };
    }

    const candidateSessionKey = parsed.sessionKey?.trim();
    if (candidateSessionKey) {
      try {
        const fetchedOrgs = await this.claudeApiService.fetchOrganizations(candidateSessionKey);
        this.organizations = fetchedOrgs;
        if (fetchedOrgs.length === 0) {
          return {
            ok: false,
            error: { code: 'VALIDATION', message: 'No organizations found for this account.' },
          };
        }

        const chosenOrgId = parsed.selectedOrganizationId?.trim()
          ? parsed.selectedOrganizationId.trim()
          : this.settingsService.getSelectedOrganizationId();

        const resolvedOrgId =
          chosenOrgId && fetchedOrgs.some((o) => o.id === chosenOrgId)
            ? chosenOrgId
            : fetchedOrgs[0]?.id;
        this.settingsService.setSelectedOrganizationId(resolvedOrgId);

        this.sessionKeyService.setInMemory(candidateSessionKey);
        if (parsed.rememberSessionKey) {
          await this.sessionKeyService.rememberKey(candidateSessionKey);
        }
      } catch (error) {
        const status = getClaudeWebRequestErrorStatus(error);
        if (status === 'unauthorized') {
          return { ok: false, error: { code: 'UNAUTHORIZED', message: 'Unauthorized.' } };
        }
        if (status === 'rate_limited') {
          return { ok: false, error: { code: 'RATE_LIMITED', message: 'Rate limited.' } };
        }
        const message =
          error instanceof Error
            ? sanitizeMessage(error.message)
            : 'Failed to validate session key.';
        return { ok: false, error: { code: 'NETWORK', message } };
      }
    }

    this.settingsService.setRefreshIntervalSeconds(refreshIntervalSeconds);
    if (!candidateSessionKey) {
      this.settingsService.setSelectedOrganizationId(parsed.selectedOrganizationId);
    }
    this.settingsService.setRememberSessionKey(Boolean(parsed.rememberSessionKey));
    this.settingsService.setNotifyOnUsageReset(Boolean(parsed.notifyOnUsageReset));

    await this.refreshNow();
    return { ok: true, value: null };
  }

  async forgetKey(): Promise<IpcResult<null>> {
    await this.sessionKeyService.forgetKey();
    this.organizations = [];
    this.updateSnapshot(this.sessionKeyService.buildMissingKeySnapshot());
    this.stop();
    return { ok: true, value: null };
  }

  async refreshNow(): Promise<IpcResult<null>> {
    if (!this.running) this.start();

    if (this.currentRun) {
      this.pendingImmediate = true;
      return { ok: true, value: null };
    }

    await this.runOnce();
    return { ok: true, value: null };
  }

  private scheduleNext(delayMs: number): void {
    if (!this.running) return;
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.tick(), delayMs);
  }

  private async tick(): Promise<void> {
    if (!this.running) return;
    if (this.currentRun) return;
    await this.runOnce();
  }

  private async runOnce(): Promise<void> {
    if (!this.running) return;
    this.currentRun = this.refreshAll().finally(() => {
      this.currentRun = null;
    });
    await this.currentRun;

    if (!this.running) return;

    if (this.pendingImmediate) {
      this.pendingImmediate = false;
      this.scheduleNext(0);
      return;
    }

    const nextDelay = this.getNextDelayMs();
    this.scheduleNext(nextDelay);
  }

  private getNextDelayMs(): number {
    const baseSeconds = Math.max(10, this.settingsService.getRefreshIntervalSeconds());
    const baseMs = baseSeconds * 1000;

    if (this.latestSnapshot?.status === 'rate_limited') {
      return withJitter(5 * 60 * 1000, 0.2);
    }

    return withJitter(baseMs, 0.1);
  }

  private updateSnapshot(snapshot: ClaudeUsageSnapshot | null): void {
    this.latestSnapshot = snapshot;
    this.trayService.updateSnapshot(snapshot);
    for (const listener of this.snapshotListeners) listener(snapshot);
  }

  private async resolveOrganizationId(sessionKey: string): Promise<string | null> {
    this.organizations = await this.claudeApiService.fetchOrganizations(sessionKey);
    const stored = this.settingsService.getSelectedOrganizationId();
    if (stored && this.organizations.some((o) => o.id === stored)) return stored;
    const first = this.organizations[0]?.id ?? null;
    if (first) this.settingsService.setSelectedOrganizationId(first);
    return first;
  }

  private async refreshAll(): Promise<void> {
    const sessionKey = await this.sessionKeyService.getCurrentKey();
    if (!sessionKey) {
      this.updateSnapshot(this.sessionKeyService.buildMissingKeySnapshot());
      this.stop();
      return;
    }

    let orgId: string | null = null;
    try {
      orgId = await this.resolveOrganizationId(sessionKey);
    } catch {
      this.updateSnapshot({
        status: 'error',
        lastUpdatedAt: nowIso(),
        errorMessage: 'Failed to fetch organizations.',
      });
      return;
    }

    if (!orgId) {
      this.updateSnapshot({
        status: 'error',
        lastUpdatedAt: nowIso(),
        errorMessage: 'No organizations found for this account.',
      });
      return;
    }

    const snapshot = await this.claudeApiService.fetchUsageSnapshot(sessionKey, orgId);
    this.updateSnapshot(snapshot);
    this.usageNotificationService.maybeNotify(snapshot);

    if (snapshot.status === 'unauthorized') {
      this.stop();
    }
  }
}
