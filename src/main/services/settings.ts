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
};

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
}
