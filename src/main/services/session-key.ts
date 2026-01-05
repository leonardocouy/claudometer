import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { type ClaudeUsageSnapshot, nowIso } from '../../shared/types.ts';

const KEYTAR_SERVICE = 'claudometer';
const KEYTAR_ACCOUNT = 'claude-sessionKey';
const FALLBACK_DIR = path.join(os.homedir(), '.claudometer');
const FALLBACK_KEY_PATH = path.join(FALLBACK_DIR, 'session-key');

type KeytarModule = {
  getPassword: (service: string, account: string) => Promise<string | null>;
  setPassword: (service: string, account: string, password: string) => Promise<void>;
  deletePassword: (service: string, account: string) => Promise<boolean>;
};

async function readFallbackKeyFile(): Promise<string | null> {
  try {
    const contents = await readFile(FALLBACK_KEY_PATH, 'utf8');
    return contents?.trim() ? contents.trim() : null;
  } catch {
    return null;
  }
}

async function writeFallbackKeyFile(sessionKey: string): Promise<void> {
  await mkdir(FALLBACK_DIR, { recursive: true, mode: 0o700 });
  await writeFile(FALLBACK_KEY_PATH, `${sessionKey}\n`, { mode: 0o600 });
}

async function deleteFallbackKeyFile(): Promise<void> {
  try {
    await rm(FALLBACK_KEY_PATH, { force: true });
  } catch {
    // ignore
  }
}

async function tryLoadKeytar(): Promise<KeytarModule | null> {
  // Linux: explicitly prefer file-based storage to avoid native toolchain/libsecret friction.
  if (process.platform === 'linux') return null;
  try {
    const mod = (await import('keytar')) as unknown as KeytarModule;
    if (typeof mod.getPassword !== 'function') return null;
    return mod;
  } catch {
    return null;
  }
}

export class SessionKeyService {
  private inMemoryKey: string | null = null;

  setInMemory(key: string | null): void {
    this.inMemoryKey = key?.trim() ? key.trim() : null;
  }

  async isKeytarAvailable(): Promise<boolean> {
    return (await tryLoadKeytar()) !== null;
  }

  async getCurrentKey(): Promise<string | null> {
    if (this.inMemoryKey) return this.inMemoryKey;
    const keytar = await tryLoadKeytar();
    if (keytar) {
      const stored = await keytar.getPassword(KEYTAR_SERVICE, KEYTAR_ACCOUNT);
      if (stored?.trim()) return stored.trim();
    }
    return readFallbackKeyFile();
  }

  async rememberKey(key: string): Promise<void> {
    const trimmed = key.trim();
    if (!trimmed) return;
    const keytar = await tryLoadKeytar();
    if (!keytar) {
      // Fallback: write to a local file with restrictive permissions.
      await writeFallbackKeyFile(trimmed);
      return;
    }
    await keytar.setPassword(KEYTAR_SERVICE, KEYTAR_ACCOUNT, trimmed);
  }

  async forgetKey(): Promise<boolean> {
    this.inMemoryKey = null;
    const keytar = await tryLoadKeytar();
    await deleteFallbackKeyFile();
    if (!keytar) return true;
    return keytar.deletePassword(KEYTAR_SERVICE, KEYTAR_ACCOUNT);
  }

  buildMissingKeySnapshot(): ClaudeUsageSnapshot {
    return {
      status: 'missing_key',
      lastUpdatedAt: nowIso(),
      errorMessage: 'Session key is not configured.',
    };
  }
}

