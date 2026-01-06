/**
 * Parser for Claude Code CLI `/usage` output.
 * Extracts usage percentages and reset times from ANSI-rich terminal output.
 */

import type { ClaudeUsageSnapshot } from './types.ts';
import { nowIso } from './types.ts';

/** Organization ID used for CLI mode (no real org in CLI) */
export const CLI_ORG_ID = 'local-cli';

/** Strip ANSI escape codes from string */
export function stripAnsi(str: string): string {
  // biome-ignore lint/suspicious/noControlCharactersInRegex: ANSI codes use control chars
  return str.replace(/\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g, '');
}

export type ParsedCliUsage = {
  sessionPercent: number;
  sessionResetsAt?: string;
  weeklyPercent: number;
  weeklyResetsAt?: string;
  models: Array<{ name: string; percent: number; resetsAt?: string }>;
};

export type CliParseResult =
  | { ok: true; data: ParsedCliUsage }
  | { ok: false; error: 'unauthorized' | 'parse_error'; message: string };

/**
 * Parse Claude CLI /usage output into structured usage data.
 *
 * Expected output patterns (after stripping ANSI):
 * - "Current session" followed by "XX% used" and "Resets <time>"
 * - "Current week (all models)" followed by "XX% used" and "Resets <time>"
 * - "Current week (Sonnet only)" followed by "XX% used" and "Resets <time>" (optional)
 *
 * Error patterns:
 * - "Error: Failed to load usage data: {...}" with permission_error
 */
export function parseCliUsageOutput(rawOutput: string): CliParseResult {
  const text = stripAnsi(rawOutput);

  // Check for OAuth/permission errors
  if (text.includes('permission_error') || text.includes('OAuth token')) {
    return {
      ok: false,
      error: 'unauthorized',
      message: 'Claude CLI authentication expired. Please re-authenticate with `claude /login`.',
    };
  }

  // Check for generic errors
  if (text.includes('Error: Failed to load usage data')) {
    const errorMatch = text.match(/Error: Failed to load usage data[:\s]*(.+?)(?:\n|$)/);
    return {
      ok: false,
      error: 'unauthorized',
      message: errorMatch?.[1]?.trim() || 'Failed to load usage data from Claude CLI.',
    };
  }

  // Parse session usage
  const sessionMatch = parseUsageBlock(text, 'Current session');
  if (!sessionMatch) {
    return {
      ok: false,
      error: 'parse_error',
      message: 'Could not parse Claude CLI usage output.',
    };
  }

  // Parse weekly (all models) usage
  const weeklyMatch = parseUsageBlock(text, 'Current week (all models)');
  if (!weeklyMatch) {
    return {
      ok: false,
      error: 'parse_error',
      message: 'Could not parse Claude CLI weekly usage output.',
    };
  }

  // Parse model-specific weekly usage (collect all available models)
  const models: Array<{ name: string; percent: number; resetsAt?: string }> = [];

  // Check for common model patterns
  const modelPatterns = [
    { pattern: 'Current week (Sonnet only)', name: 'Sonnet' },
    { pattern: 'Current week (Opus only)', name: 'Opus' },
    { pattern: 'Current week (Haiku only)', name: 'Haiku' },
  ];

  for (const { pattern, name } of modelPatterns) {
    const modelMatch = parseUsageBlock(text, pattern);
    if (modelMatch) {
      models.push({
        name,
        percent: modelMatch.percent,
        resetsAt: modelMatch.resetsAt,
      });
    }
  }

  return {
    ok: true,
    data: {
      sessionPercent: sessionMatch.percent,
      sessionResetsAt: sessionMatch.resetsAt,
      weeklyPercent: weeklyMatch.percent,
      weeklyResetsAt: weeklyMatch.resetsAt,
      models,
    },
  };
}

type UsageBlock = { percent: number; resetsAt?: string };

/**
 * Parse a usage block from the text.
 * Looks for the header, then finds "XX% used" and "Resets <time>" nearby.
 */
function parseUsageBlock(text: string, header: string): UsageBlock | null {
  const headerIndex = text.indexOf(header);
  if (headerIndex === -1) return null;

  // Look for content after the header (next 500 chars should be enough)
  const blockText = text.slice(headerIndex, headerIndex + 500);

  // Match percentage: "8% used" or "32% used"
  const percentMatch = blockText.match(/(\d+)%\s*used/);
  if (!percentMatch) return null;

  const percent = Number.parseInt(percentMatch[1], 10);
  if (!Number.isFinite(percent)) return null;

  // Match reset time: "Resets 11:59am (America/Sao_Paulo)" or "Resets Jan 9, 12:59pm (America/Sao_Paulo)"
  const resetsMatch = blockText.match(/Resets\s+([^(\n]+(?:\([^)]+\))?)/);
  const resetsAt = resetsMatch?.[1]?.trim();

  return { percent, resetsAt };
}

/**
 * Convert parsed CLI usage to a ClaudeUsageSnapshot.
 */
export function cliUsageToSnapshot(parsed: ParsedCliUsage): ClaudeUsageSnapshot {
  return {
    status: 'ok',
    organizationId: CLI_ORG_ID,
    sessionPercent: parsed.sessionPercent,
    sessionResetsAt: parsed.sessionResetsAt,
    weeklyPercent: parsed.weeklyPercent,
    weeklyResetsAt: parsed.weeklyResetsAt,
    models: parsed.models,
    lastUpdatedAt: nowIso(),
  };
}

/**
 * Create an error snapshot for CLI failures.
 */
export function cliErrorSnapshot(
  status: 'unauthorized' | 'error',
  message: string,
): ClaudeUsageSnapshot {
  return {
    status,
    organizationId: CLI_ORG_ID,
    lastUpdatedAt: nowIso(),
    errorMessage: message,
  };
}
