import { describe, expect, test } from 'bun:test';
import { buildBeepWavBytes, buildLinuxSoundAttempts } from './notificationSound.ts';

describe('notificationSound', () => {
  test('buildBeepWavBytes produces a valid WAV header', () => {
    const bytes = buildBeepWavBytes({ durationMs: 50 });
    expect(bytes.length).toBeGreaterThan(44);

    const read4 = (offset: number) =>
      String.fromCharCode(
        bytes[offset] ?? 0,
        bytes[offset + 1] ?? 0,
        bytes[offset + 2] ?? 0,
        bytes[offset + 3] ?? 0,
      );

    const riff = read4(0);
    const wave = read4(8);
    const fmt = read4(12);
    const data = read4(36);

    expect(riff).toBe('RIFF');
    expect(wave).toBe('WAVE');
    expect(fmt).toBe('fmt ');
    expect(data).toBe('data');
  });

  test('buildLinuxSoundAttempts includes canberra then file-based players', () => {
    const attempts = buildLinuxSoundAttempts('/tmp/alert.wav');
    expect(attempts[0]?.command).toBe('canberra-gtk-play');
    expect(attempts[0]?.args).toEqual(['-i', 'message-new-instant']);

    const commands = attempts.map((a) => a.command);
    expect(commands).toContain('paplay');
    expect(commands).toContain('pw-play');
    expect(commands).toContain('aplay');
  });
});
