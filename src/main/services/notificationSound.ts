import { spawn as nodeSpawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export type SoundSpawn = typeof nodeSpawn;

export type SoundAttempt = {
  command: string;
  args: string[];
};

export function buildLinuxSoundAttempts(alertWavPath: string): SoundAttempt[] {
  return [
    { command: 'canberra-gtk-play', args: ['-i', 'message-new-instant'] },
    { command: 'paplay', args: [alertWavPath] },
    { command: 'pw-play', args: [alertWavPath] },
    { command: 'aplay', args: ['-q', alertWavPath] },
  ];
}

function writeLittleEndianUInt32(buf: Uint8Array, offset: number, value: number): void {
  buf[offset + 0] = value & 0xff;
  buf[offset + 1] = (value >> 8) & 0xff;
  buf[offset + 2] = (value >> 16) & 0xff;
  buf[offset + 3] = (value >> 24) & 0xff;
}

function writeLittleEndianUInt16(buf: Uint8Array, offset: number, value: number): void {
  buf[offset + 0] = value & 0xff;
  buf[offset + 1] = (value >> 8) & 0xff;
}

function encodeAscii(buf: Uint8Array, offset: number, value: string): void {
  for (let i = 0; i < value.length; i++) buf[offset + i] = value.charCodeAt(i) & 0xff;
}

export function buildBeepWavBytes(params?: {
  durationMs?: number;
  sampleRateHz?: number;
  frequencyHz?: number;
  volume?: number; // 0..1
}): Uint8Array {
  const durationMs = params?.durationMs ?? 120;
  const sampleRateHz = params?.sampleRateHz ?? 44100;
  const frequencyHz = params?.frequencyHz ?? 880;
  const volume = Math.max(0, Math.min(1, params?.volume ?? 0.25));

  const channels = 1;
  const bitsPerSample = 16;
  const bytesPerSample = bitsPerSample / 8;
  const numSamples = Math.max(1, Math.floor((durationMs / 1000) * sampleRateHz));
  const dataSize = numSamples * channels * bytesPerSample;

  const headerSize = 44;
  const totalSize = headerSize + dataSize;
  const bytes = new Uint8Array(totalSize);

  // RIFF header
  encodeAscii(bytes, 0, 'RIFF');
  writeLittleEndianUInt32(bytes, 4, totalSize - 8);
  encodeAscii(bytes, 8, 'WAVE');

  // fmt chunk
  encodeAscii(bytes, 12, 'fmt ');
  writeLittleEndianUInt32(bytes, 16, 16); // PCM
  writeLittleEndianUInt16(bytes, 20, 1); // audio format = PCM
  writeLittleEndianUInt16(bytes, 22, channels);
  writeLittleEndianUInt32(bytes, 24, sampleRateHz);
  const byteRate = sampleRateHz * channels * bytesPerSample;
  writeLittleEndianUInt32(bytes, 28, byteRate);
  const blockAlign = channels * bytesPerSample;
  writeLittleEndianUInt16(bytes, 32, blockAlign);
  writeLittleEndianUInt16(bytes, 34, bitsPerSample);

  // data chunk
  encodeAscii(bytes, 36, 'data');
  writeLittleEndianUInt32(bytes, 40, dataSize);

  // PCM data (16-bit signed little endian)
  const amplitude = Math.floor(32767 * volume);
  for (let i = 0; i < numSamples; i++) {
    const t = i / sampleRateHz;
    const fade = Math.min(
      1,
      Math.min(i / (sampleRateHz * 0.01), (numSamples - i) / (sampleRateHz * 0.01)),
    );
    const sample = Math.sin(2 * Math.PI * frequencyHz * t) * amplitude * fade;
    const clamped = Math.max(-32768, Math.min(32767, Math.round(sample)));
    const offset = headerSize + i * 2;
    bytes[offset + 0] = clamped & 0xff;
    bytes[offset + 1] = (clamped >> 8) & 0xff;
  }

  return bytes;
}

export class NotificationSoundService {
  private platform: NodeJS.Platform;
  private userDataDir: string;
  private spawn: SoundSpawn;
  private inFlight = false;
  private lastStartedAtMs = 0;

  constructor(params: { platform: NodeJS.Platform; userDataDir: string; spawn?: SoundSpawn }) {
    this.platform = params.platform;
    this.userDataDir = params.userDataDir;
    this.spawn = params.spawn ?? nodeSpawn;
  }

  playAlertSound(): void {
    if (this.platform !== 'linux') return;

    const now = Date.now();
    if (this.inFlight) return;
    if (now - this.lastStartedAtMs < 750) return;

    this.inFlight = true;
    this.lastStartedAtMs = now;
    void this.playLinuxAlertSound().finally(() => {
      this.inFlight = false;
    });
  }

  private ensureAlertWavPath(): string | null {
    try {
      const dir = path.join(this.userDataDir, 'sounds');
      fs.mkdirSync(dir, { recursive: true });
      const filePath = path.join(dir, 'claudometer-alert.wav');
      if (!fs.existsSync(filePath)) {
        const wavBytes = buildBeepWavBytes();
        fs.writeFileSync(filePath, wavBytes);
      }
      return filePath;
    } catch {
      return null;
    }
  }

  private async playLinuxAlertSound(): Promise<void> {
    const wavPath = this.ensureAlertWavPath();
    if (!wavPath) return;

    const attempts = buildLinuxSoundAttempts(wavPath);
    for (const attempt of attempts) {
      const ok = await this.trySpawnAttempt(attempt, 900);
      if (ok) return;
    }
  }

  private trySpawnAttempt(attempt: SoundAttempt, timeoutMs: number): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      let settled = false;
      const finish = (value: boolean) => {
        if (settled) return;
        settled = true;
        resolve(value);
      };

      const timer = setTimeout(() => finish(false), timeoutMs);

      try {
        const child = this.spawn(attempt.command, attempt.args, {
          stdio: 'ignore',
        });

        child.once('error', () => {
          clearTimeout(timer);
          finish(false);
        });

        child.once('exit', (code) => {
          clearTimeout(timer);
          finish(code === 0);
        });

        child.unref();
      } catch {
        clearTimeout(timer);
        finish(false);
      }
    });
  }
}
