import { describe, expect, test } from 'bun:test';
import { mapHttpStatusToUsageStatus, parseClaudeUsageFromJson } from './parser.ts';

describe('parseClaudeUsageFromJson', () => {
  test('parses numeric utilization and resets_at', () => {
    const json = {
      five_hour: { utilization: 12.3, resets_at: '2026-01-01T12:00:00.000Z' },
      seven_day: { utilization: 55, resets_at: '2026-01-08T12:00:00.000Z' },
      seven_day_opus: { utilization: 9, resets_at: '2026-01-08T12:00:00.000Z' },
    };

    const snapshot = parseClaudeUsageFromJson(json, 'org-1', '2026-01-01T00:00:00.000Z');

    expect(snapshot.status).toBe('ok');
    expect(snapshot.organizationId).toBe('org-1');
    expect(snapshot.sessionPercent).toBeCloseTo(12.3);
    expect(snapshot.sessionResetsAt).toBe('2026-01-01T12:00:00.000Z');
    expect(snapshot.weeklyPercent).toBe(55);
    expect(snapshot.weeklyResetsAt).toBe('2026-01-08T12:00:00.000Z');
    expect(snapshot.modelWeeklyName).toBe('Opus');
    expect(snapshot.modelWeeklyPercent).toBe(9);
    expect(snapshot.modelWeeklyResetsAt).toBe('2026-01-08T12:00:00.000Z');
  });

  test('handles string utilization and missing fields', () => {
    const json = {
      five_hour: { utilization: ' 80.5 ' },
      seven_day: {},
      seven_day_opus: { utilization: '10' },
    };

    const snapshot = parseClaudeUsageFromJson(json, 'org-2', '2026-01-01T00:00:00.000Z');

    expect(snapshot.sessionPercent).toBeCloseTo(80.5);
    expect(snapshot.sessionResetsAt).toBeUndefined();
    expect(snapshot.weeklyPercent).toBe(0);
    expect(snapshot.modelWeeklyName).toBe('Opus');
    expect(snapshot.modelWeeklyPercent).toBe(10);
  });

  test('clamps utilization to 0..100', () => {
    const json = {
      five_hour: { utilization: 999 },
      seven_day: { utilization: -10 },
      seven_day_opus: { utilization: 101 },
    };

    const snapshot = parseClaudeUsageFromJson(json, 'org-3', '2026-01-01T00:00:00.000Z');

    expect(snapshot.sessionPercent).toBe(100);
    expect(snapshot.weeklyPercent).toBe(0);
    expect(snapshot.modelWeeklyName).toBe('Opus');
    expect(snapshot.modelWeeklyPercent).toBe(100);
  });

  test('prefers seven_day_sonnet when present', () => {
    const json = {
      five_hour: { utilization: 1 },
      seven_day: { utilization: 2 },
      seven_day_opus: { utilization: 9 },
      seven_day_sonnet: { utilization: 20, resets_at: '2026-01-09T16:00:00.313070+00:00' },
    };

    const snapshot = parseClaudeUsageFromJson(json, 'org-4', '2026-01-01T00:00:00.000Z');
    expect(snapshot.modelWeeklyName).toBe('Sonnet');
    expect(snapshot.modelWeeklyPercent).toBe(20);
    expect(snapshot.modelWeeklyResetsAt).toBe('2026-01-09T16:00:00.313070+00:00');
  });
});

describe('mapHttpStatusToUsageStatus', () => {
  test('maps 401/403 to unauthorized', () => {
    expect(mapHttpStatusToUsageStatus(401)).toBe('unauthorized');
    expect(mapHttpStatusToUsageStatus(403)).toBe('unauthorized');
  });

  test('maps 429 to rate_limited', () => {
    expect(mapHttpStatusToUsageStatus(429)).toBe('rate_limited');
  });

  test('maps others to error', () => {
    expect(mapHttpStatusToUsageStatus(500)).toBe('error');
    expect(mapHttpStatusToUsageStatus(418)).toBe('error');
  });
});
