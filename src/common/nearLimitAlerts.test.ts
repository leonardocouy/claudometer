import { describe, expect, test } from 'bun:test';
import { decideNearLimitAlerts } from './nearLimitAlerts.ts';

describe('decideNearLimitAlerts', () => {
  test('does not notify below threshold', () => {
    const result = decideNearLimitAlerts({
      currentSessionPercent: 89.9,
      currentWeeklyPercent: 0,
      currentSessionResetsAt: '2026-01-01T05:00:00.000Z',
      currentWeeklyResetsAt: '2026-01-08T00:00:00.000Z',
    });

    expect(result.notifySession).toBe(false);
    expect(result.notifyWeekly).toBe(false);
  });

  test('notifies once when first observed >= 90% and not yet notified for that period', () => {
    const result = decideNearLimitAlerts({
      currentSessionPercent: 90,
      currentWeeklyPercent: 95,
      currentSessionResetsAt: '2026-01-01T05:00:00.000Z',
      currentWeeklyResetsAt: '2026-01-08T00:00:00.000Z',
      lastNotifiedSessionPeriodId: '',
      lastNotifiedWeeklyPeriodId: '',
    });

    expect(result.notifySession).toBe(true);
    expect(result.sessionPeriodId).toBe('2026-01-01T05:00:00.000Z');
    expect(result.notifyWeekly).toBe(true);
    expect(result.weeklyPeriodId).toBe('2026-01-08T00:00:00.000Z');
  });

  test('does not notify again for the same period id', () => {
    const result = decideNearLimitAlerts({
      currentSessionPercent: 99,
      currentWeeklyPercent: 99,
      currentSessionResetsAt: '2026-01-01T05:00:00.000Z',
      currentWeeklyResetsAt: '2026-01-08T00:00:00.000Z',
      previousSessionPercent: 95,
      previousWeeklyPercent: 95,
      lastNotifiedSessionPeriodId: '2026-01-01T05:00:00.000Z',
      lastNotifiedWeeklyPeriodId: '2026-01-08T00:00:00.000Z',
    });

    expect(result.notifySession).toBe(false);
    expect(result.notifyWeekly).toBe(false);
  });

  test('allows notification again when period id changes', () => {
    const result = decideNearLimitAlerts({
      currentSessionPercent: 92,
      currentWeeklyPercent: 92,
      currentSessionResetsAt: '2026-01-01T10:00:00.000Z',
      currentWeeklyResetsAt: '2026-01-15T00:00:00.000Z',
      previousSessionPercent: 10,
      previousWeeklyPercent: 10,
      lastNotifiedSessionPeriodId: '2026-01-01T05:00:00.000Z',
      lastNotifiedWeeklyPeriodId: '2026-01-08T00:00:00.000Z',
    });

    expect(result.notifySession).toBe(true);
    expect(result.sessionPeriodId).toBe('2026-01-01T10:00:00.000Z');
    expect(result.notifyWeekly).toBe(true);
    expect(result.weeklyPeriodId).toBe('2026-01-15T00:00:00.000Z');
  });

  test('uses an unknown period id when resets_at is missing (still once)', () => {
    const first = decideNearLimitAlerts({
      currentSessionPercent: 90,
      currentWeeklyPercent: 90,
      lastNotifiedSessionPeriodId: '',
      lastNotifiedWeeklyPeriodId: '',
    });
    expect(first.notifySession).toBe(true);
    expect(first.sessionPeriodId).toBe('unknown');
    expect(first.notifyWeekly).toBe(true);
    expect(first.weeklyPeriodId).toBe('unknown');

    const second = decideNearLimitAlerts({
      currentSessionPercent: 95,
      currentWeeklyPercent: 95,
      previousSessionPercent: 90,
      previousWeeklyPercent: 90,
      lastNotifiedSessionPeriodId: 'unknown',
      lastNotifiedWeeklyPeriodId: 'unknown',
    });
    expect(second.notifySession).toBe(false);
    expect(second.notifyWeekly).toBe(false);
  });
});
