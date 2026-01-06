/**
 * Tests for Claude CLI /usage output parser
 */

import { describe, expect, test } from 'bun:test';
import {
  CLI_ORG_ID,
  cliErrorSnapshot,
  cliUsageToSnapshot,
  parseCliUsageOutput,
  stripAnsi,
} from './cliParser.ts';

describe('stripAnsi', () => {
  test('removes ANSI escape codes', () => {
    const input = '\x1b[38;2;255;255;255mHello\x1b[0m World';
    expect(stripAnsi(input)).toBe('Hello World');
  });

  test('handles text without ANSI codes', () => {
    const input = 'Plain text';
    expect(stripAnsi(input)).toBe('Plain text');
  });

  test('removes complex ANSI sequences', () => {
    const input = '\x1b[1mBold\x1b[22m \x1b[38;2;153;153;153mGray\x1b[39m';
    expect(stripAnsi(input)).toBe('Bold Gray');
  });
});

describe('parseCliUsageOutput', () => {
  // Sample successful output (ANSI stripped)
  const successOutput = `
> /usage
────────────────────────────────────────────────────────────────────────────────
 Settings:  Status   Config   Usage  (tab to cycle)

 Current session
 ████                                               8% used
 Resets 11:59am (America/Sao_Paulo)

 Current week (all models)
 ████████████████                                   32% used
 Resets Jan 9, 12:59pm (America/Sao_Paulo)

 Current week (Sonnet only)
 ███████████▌                                       23% used
 Resets Jan 9, 12:59pm (America/Sao_Paulo)

 Extra usage
 Extra usage not enabled • /extra-usage to enable

 Esc to cancel
`;

  test('parses successful output correctly', () => {
    const result = parseCliUsageOutput(successOutput);
    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.data.sessionPercent).toBe(8);
    expect(result.data.sessionResetsAt).toBe('11:59am (America/Sao_Paulo)');
    expect(result.data.weeklyPercent).toBe(32);
    expect(result.data.weeklyResetsAt).toBe('Jan 9, 12:59pm (America/Sao_Paulo)');
    expect(result.data.modelWeeklyPercent).toBe(23);
    expect(result.data.modelWeeklyName).toBe('Sonnet');
    expect(result.data.modelWeeklyResetsAt).toBe('Jan 9, 12:59pm (America/Sao_Paulo)');
  });

  test('parses output without model-specific weekly', () => {
    const outputWithoutSonnet = `
 Current session
 ████                                               50% used
 Resets 3:00pm (UTC)

 Current week (all models)
 ████████████████                                   75% used
 Resets Jan 10, 12:00am (UTC)
`;
    const result = parseCliUsageOutput(outputWithoutSonnet);
    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.data.sessionPercent).toBe(50);
    expect(result.data.weeklyPercent).toBe(75);
    expect(result.data.modelWeeklyPercent).toBeUndefined();
    expect(result.data.modelWeeklyName).toBeUndefined();
  });

  test('handles OAuth/permission error', () => {
    const errorOutput = `
> /usage
────────────────────────────────────────────────────────────────────────────────
 Settings:  Status   Config   Usage  (tab to cycle)

 Error: Failed to load usage data:
 {"type":"error","error":{"type":"permission_error","message":"OAuth token does
 not meet scope requirement user:profile","details":{"error_visibility":"user_
 facing"}},"request_id":"req_011CWr1wC7a15jxWR1BarauX"}

 r to retry · Esc to cancel
`;
    const result = parseCliUsageOutput(errorOutput);
    expect(result.ok).toBe(false);
    if (result.ok) return;

    expect(result.error).toBe('unauthorized');
    expect(result.message).toContain('authentication');
  });

  test('handles generic error message', () => {
    const errorOutput = `
 Error: Failed to load usage data: Network timeout
`;
    const result = parseCliUsageOutput(errorOutput);
    expect(result.ok).toBe(false);
    if (result.ok) return;

    expect(result.error).toBe('unauthorized');
    expect(result.message).toContain('Network timeout');
  });

  test('returns parse_error for missing session data', () => {
    const badOutput = `
 Current week (all models)
 ████████████████                                   32% used
`;
    const result = parseCliUsageOutput(badOutput);
    expect(result.ok).toBe(false);
    if (result.ok) return;

    expect(result.error).toBe('parse_error');
    expect(result.message).toContain('parse');
  });

  test('returns parse_error for missing weekly data', () => {
    const badOutput = `
 Current session
 ████                                               8% used
 Resets 11:59am (America/Sao_Paulo)
`;
    const result = parseCliUsageOutput(badOutput);
    expect(result.ok).toBe(false);
    if (result.ok) return;

    expect(result.error).toBe('parse_error');
    expect(result.message).toContain('weekly');
  });

  test('handles 0% usage', () => {
    const zeroOutput = `
 Current session
 ████                                               0% used
 Resets 11:59am (UTC)

 Current week (all models)
 ████████████████                                   0% used
 Resets Jan 9, 12:59pm (UTC)
`;
    const result = parseCliUsageOutput(zeroOutput);
    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.data.sessionPercent).toBe(0);
    expect(result.data.weeklyPercent).toBe(0);
  });

  test('handles 100% usage', () => {
    const fullOutput = `
 Current session
 ████████████████████████████████████████████████   100% used
 Resets 11:59am (UTC)

 Current week (all models)
 ████████████████████████████████████████████████   100% used
 Resets Jan 9, 12:59pm (UTC)
`;
    const result = parseCliUsageOutput(fullOutput);
    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.data.sessionPercent).toBe(100);
    expect(result.data.weeklyPercent).toBe(100);
  });
});

describe('cliUsageToSnapshot', () => {
  test('creates snapshot with correct structure', () => {
    const parsed = {
      sessionPercent: 8,
      sessionResetsAt: '11:59am (America/Sao_Paulo)',
      weeklyPercent: 32,
      weeklyResetsAt: 'Jan 9, 12:59pm (America/Sao_Paulo)',
      modelWeeklyPercent: 23,
      modelWeeklyName: 'Sonnet' as const,
      modelWeeklyResetsAt: 'Jan 9, 12:59pm (America/Sao_Paulo)',
    };

    const snapshot = cliUsageToSnapshot(parsed);

    expect(snapshot.status).toBe('ok');
    expect(snapshot.organizationId).toBe(CLI_ORG_ID);
    if (snapshot.status !== 'ok') return;

    expect(snapshot.sessionPercent).toBe(8);
    expect(snapshot.sessionResetsAt).toBe('11:59am (America/Sao_Paulo)');
    expect(snapshot.weeklyPercent).toBe(32);
    expect(snapshot.weeklyResetsAt).toBe('Jan 9, 12:59pm (America/Sao_Paulo)');
    expect(snapshot.modelWeeklyPercent).toBe(23);
    expect(snapshot.modelWeeklyName).toBe('Sonnet');
    expect(snapshot.modelWeeklyResetsAt).toBe('Jan 9, 12:59pm (America/Sao_Paulo)');
    expect(snapshot.lastUpdatedAt).toBeDefined();
  });

  test('handles missing optional fields', () => {
    const parsed = {
      sessionPercent: 50,
      weeklyPercent: 75,
    };

    const snapshot = cliUsageToSnapshot(parsed);

    expect(snapshot.status).toBe('ok');
    if (snapshot.status !== 'ok') return;

    expect(snapshot.sessionPercent).toBe(50);
    expect(snapshot.weeklyPercent).toBe(75);
    expect(snapshot.modelWeeklyPercent).toBe(0);
    expect(snapshot.modelWeeklyName).toBeUndefined();
  });
});

describe('cliErrorSnapshot', () => {
  test('creates unauthorized error snapshot', () => {
    const snapshot = cliErrorSnapshot('unauthorized', 'Auth expired');

    expect(snapshot.status).toBe('unauthorized');
    expect(snapshot.organizationId).toBe(CLI_ORG_ID);
    if (snapshot.status !== 'ok') {
      expect(snapshot.errorMessage).toBe('Auth expired');
    }
    expect(snapshot.lastUpdatedAt).toBeDefined();
  });

  test('creates error snapshot', () => {
    const snapshot = cliErrorSnapshot('error', 'CLI not found');

    expect(snapshot.status).toBe('error');
    expect(snapshot.organizationId).toBe(CLI_ORG_ID);
    if (snapshot.status !== 'ok') {
      expect(snapshot.errorMessage).toBe('CLI not found');
    }
  });
});
