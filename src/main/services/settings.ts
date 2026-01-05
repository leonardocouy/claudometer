/**
 * Settings Manager - Persistent user preferences using electron-store
 */

import Store from 'electron-store';

export interface AppSettings {
  refreshIntervalSeconds: number;
  selectedOrganizationId?: string;
  rememberSessionKey: boolean;
  /**
   * Encrypted ciphertext of Claude `sessionKey`, base64-encoded.
   * Plaintext MUST never be persisted.
   */
  sessionKeyEncryptedB64?: string;
  /**
   * Marker for "already notified near-limit" per organization for the session window.
   * Value is the `resets_at` string (or "unknown" if missing).
   */
  sessionNearLimitNotifiedPeriodIdByOrg?: Record<string, string>;
  /**
   * Marker for "already notified near-limit" per organization for the weekly window.
   * Value is the `resets_at` string (or "unknown" if missing).
   */
  weeklyNearLimitNotifiedPeriodIdByOrg?: Record<string, string>;
}

const schema = {
  refreshIntervalSeconds: {
    type: 'number' as const,
    default: 60,
    minimum: 10,
  },
  selectedOrganizationId: {
    type: 'string' as const,
    default: '',
  },
  rememberSessionKey: {
    type: 'boolean' as const,
    default: false,
  },
  sessionKeyEncryptedB64: {
    type: 'string' as const,
    default: '',
  },
  sessionNearLimitNotifiedPeriodIdByOrg: {
    type: 'object' as const,
    default: {},
  },
  weeklyNearLimitNotifiedPeriodIdByOrg: {
    type: 'object' as const,
    default: {},
  },
};

function readStringMap(value: unknown): Record<string, string> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  const record = value as Record<string, unknown>;
  const mapped: Record<string, string> = {};
  for (const [key, entry] of Object.entries(record)) {
    if (!key.trim()) continue;
    if (typeof entry !== 'string') continue;
    if (!entry.trim()) continue;
    mapped[key] = entry;
  }
  return mapped;
}

export class SettingsService {
  private store: Store<AppSettings>;

  constructor() {
    this.store = new Store<AppSettings>({
      schema,
      name: 'claudometer-settings',
    });
  }

  getRefreshIntervalSeconds(): number {
    return this.store.get('refreshIntervalSeconds', 60);
  }

  setRefreshIntervalSeconds(seconds: number): void {
    this.store.set('refreshIntervalSeconds', seconds);
  }

  getSelectedOrganizationId(): string | undefined {
    const value = this.store.get('selectedOrganizationId', '');
    return value.trim() ? value : undefined;
  }

  setSelectedOrganizationId(orgId: string | undefined): void {
    this.store.set('selectedOrganizationId', orgId ?? '');
  }

  getRememberSessionKey(): boolean {
    return this.store.get('rememberSessionKey', false);
  }

  setRememberSessionKey(remember: boolean): void {
    this.store.set('rememberSessionKey', remember);
  }

  getSessionKeyEncryptedB64(): string | null {
    const value = this.store.get('sessionKeyEncryptedB64', '');
    return value.trim() ? value : null;
  }

  setSessionKeyEncryptedB64(value: string): void {
    this.store.set('sessionKeyEncryptedB64', value);
  }

  clearSessionKeyEncryptedB64(): void {
    this.store.set('sessionKeyEncryptedB64', '');
  }

  getSessionNearLimitNotifiedPeriodId(organizationId: string): string | null {
    if (!organizationId.trim()) return null;
    const map = readStringMap(this.store.get('sessionNearLimitNotifiedPeriodIdByOrg', {}));
    return map[organizationId] ?? null;
  }

  setSessionNearLimitNotifiedPeriodId(organizationId: string, periodId: string): void {
    const org = organizationId.trim();
    const pid = periodId.trim();
    if (!org || !pid) return;
    const map = readStringMap(this.store.get('sessionNearLimitNotifiedPeriodIdByOrg', {}));
    map[org] = pid;
    this.store.set('sessionNearLimitNotifiedPeriodIdByOrg', map);
  }

  getWeeklyNearLimitNotifiedPeriodId(organizationId: string): string | null {
    if (!organizationId.trim()) return null;
    const map = readStringMap(this.store.get('weeklyNearLimitNotifiedPeriodIdByOrg', {}));
    return map[organizationId] ?? null;
  }

  setWeeklyNearLimitNotifiedPeriodId(organizationId: string, periodId: string): void {
    const org = organizationId.trim();
    const pid = periodId.trim();
    if (!org || !pid) return;
    const map = readStringMap(this.store.get('weeklyNearLimitNotifiedPeriodIdByOrg', {}));
    map[org] = pid;
    this.store.set('weeklyNearLimitNotifiedPeriodIdByOrg', map);
  }
}
