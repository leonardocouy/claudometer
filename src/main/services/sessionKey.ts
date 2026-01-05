import { safeStorage } from 'electron';
import { type ClaudeUsageSnapshot, nowIso } from '../../common/types.ts';
import { SettingsService } from './settings.ts';

export class SessionKeyService {
  private inMemoryKey: string | null = null;
  private settingsService: SettingsService;

  constructor(settingsService: SettingsService) {
    this.settingsService = settingsService;
  }

  setInMemory(key: string | null): void {
    this.inMemoryKey = key?.trim() ? key.trim() : null;
  }

  isEncryptionAvailable(): boolean {
    return safeStorage.isEncryptionAvailable();
  }

  async getCurrentKey(): Promise<string | null> {
    if (this.inMemoryKey) return this.inMemoryKey;

    if (!this.isEncryptionAvailable()) return null;

    const b64 = this.settingsService.getSessionKeyEncryptedB64();
    if (!b64) return null;

    try {
      const decrypted = safeStorage.decryptString(Buffer.from(b64, 'base64')).trim();
      if (!decrypted) return null;
      this.inMemoryKey = decrypted;
      return decrypted;
    } catch {
      this.settingsService.clearSessionKeyEncryptedB64();
      return null;
    }
  }

  async rememberKey(key: string): Promise<void> {
    const trimmed = key.trim();
    if (!trimmed) return;
    this.inMemoryKey = trimmed;

    if (!this.isEncryptionAvailable()) return;

    const encrypted = safeStorage.encryptString(trimmed);
    this.settingsService.setSessionKeyEncryptedB64(encrypted.toString('base64'));
  }

  async forgetKey(): Promise<boolean> {
    this.inMemoryKey = null;
    this.settingsService.clearSessionKeyEncryptedB64();
    return true;
  }

  buildMissingKeySnapshot(): ClaudeUsageSnapshot {
    return {
      status: 'missing_key',
      lastUpdatedAt: nowIso(),
      errorMessage: 'Session key is not configured.',
    };
  }
}
