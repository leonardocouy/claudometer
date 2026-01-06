/**
 * Claude OAuth API Client
 * Fetches usage data from Anthropic's OAuth API using credentials from ~/.claude/.credentials.json
 */

import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';
import { nowIso } from '../../common/types.ts';

interface OAuthCredentials {
  claudeAiOauth?: {
    accessToken: string;
    refreshToken: string;
    expiresAt: number;
  };
  apiKey?: string;
}

interface UsageApiResponse {
  five_hour?: {
    units_used: number;
    units_limit: number;
    resets_at?: string;
  };
  seven_day?: {
    units_used: number;
    units_limit: number;
    resets_at?: string;
  };
  seven_day_opus?: {
    units_used: number;
    units_limit: number;
    resets_at?: string;
  } | null;
  seven_day_sonnet?: {
    units_used: number;
    units_limit: number;
    resets_at?: string;
  } | null;
}

function toPercent(used: number | undefined, limit: number | undefined): number {
  if (typeof used !== 'number' || typeof limit !== 'number' || limit <= 0) {
    return 0;
  }
  return Math.round((used / limit) * 100);
}

function errorSnapshot(status: 'error' | 'unauthorized', message: string): ClaudeUsageSnapshot {
  return {
    status,
    lastUpdatedAt: nowIso(),
    errorMessage: message,
  };
}

/**
 * Read OAuth credentials from ~/.claude/.credentials.json
 */
async function readCredentials(): Promise<OAuthCredentials | null> {
  try {
    const credentialsPath = join(homedir(), '.claude', '.credentials.json');
    const content = await readFile(credentialsPath, 'utf-8');
    const parsed = JSON.parse(content) as OAuthCredentials;
    return parsed;
  } catch (error) {
    console.error('[claudeOAuthApi] Failed to read credentials:', error);
    return null;
  }
}

/**
 * Fetch usage data from Anthropic OAuth API
 */
async function fetchUsageFromApi(accessToken: string): Promise<UsageApiResponse | null> {
  try {
    console.log('[claudeOAuthApi] Fetching usage from API...');
    const response = await fetch('https://api.anthropic.com/api/oauth/usage', {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${accessToken}`,
        'anthropic-beta': 'oauth-2025-04-20',
      },
    });

    console.log('[claudeOAuthApi] Response status:', response.status);

    if (!response.ok) {
      if (response.status === 401) {
        console.error('[claudeOAuthApi] Unauthorized (401)');
        return null;
      }
      console.error(`[claudeOAuthApi] API error: ${response.status} ${response.statusText}`);
      return null;
    }

    const data = (await response.json()) as UsageApiResponse;
    console.log('[claudeOAuthApi] API Response:', JSON.stringify(data, null, 2));
    return data;
  } catch (error) {
    console.error('[claudeOAuthApi] Network error:', error);
    return null;
  }
}

/**
 * Convert API response to ClaudeUsageSnapshot
 */
function apiResponseToSnapshot(data: UsageApiResponse): ClaudeUsageSnapshot {
  console.log('[claudeOAuthApi] Converting API response to snapshot...');

  // Session (5-hour) usage
  const sessionPercent = toPercent(data.five_hour?.units_used, data.five_hour?.units_limit);
  const sessionResetsAt = data.five_hour?.resets_at;
  console.log('[claudeOAuthApi] Session:', {
    used: data.five_hour?.units_used,
    limit: data.five_hour?.units_limit,
    percent: sessionPercent
  });

  // Weekly (7-day) usage
  const weeklyPercent = toPercent(data.seven_day?.units_used, data.seven_day?.units_limit);
  const weeklyResetsAt = data.seven_day?.resets_at;
  console.log('[claudeOAuthApi] Weekly:', {
    used: data.seven_day?.units_used,
    limit: data.seven_day?.units_limit,
    percent: weeklyPercent
  });

  // Model-specific weekly usage (fallback: Opus → Sonnet → 0)
  let modelWeeklyPercent = 0;
  let modelWeeklyName: string | undefined;
  let modelWeeklyResetsAt: string | undefined;

  if (data.seven_day_opus) {
    modelWeeklyPercent = toPercent(
      data.seven_day_opus.units_used,
      data.seven_day_opus.units_limit,
    );
    modelWeeklyName = 'Opus';
    modelWeeklyResetsAt = data.seven_day_opus.resets_at;
    console.log('[claudeOAuthApi] Model (Opus):', {
      used: data.seven_day_opus.units_used,
      limit: data.seven_day_opus.units_limit,
      percent: modelWeeklyPercent
    });
  } else if (data.seven_day_sonnet) {
    modelWeeklyPercent = toPercent(
      data.seven_day_sonnet.units_used,
      data.seven_day_sonnet.units_limit,
    );
    modelWeeklyName = 'Sonnet';
    modelWeeklyResetsAt = data.seven_day_sonnet.resets_at;
    console.log('[claudeOAuthApi] Model (Sonnet):', {
      used: data.seven_day_sonnet.units_used,
      limit: data.seven_day_sonnet.units_limit,
      percent: modelWeeklyPercent
    });
  }

  const snapshot = {
    status: 'ok' as const,
    organizationId: 'oauth', // OAuth API doesn't have organization concept
    sessionPercent,
    sessionResetsAt,
    weeklyPercent,
    weeklyResetsAt,
    modelWeeklyPercent,
    modelWeeklyName,
    modelWeeklyResetsAt,
    lastUpdatedAt: nowIso(),
  };

  console.log('[claudeOAuthApi] Final snapshot:', snapshot);
  return snapshot;
}

/**
 * Fetch usage snapshot using OAuth credentials
 */
export async function fetchOAuthUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
  console.log('[claudeOAuthApi] fetchOAuthUsageSnapshot called');

  // Step 1: Read credentials
  const credentials = await readCredentials();

  if (!credentials?.claudeAiOauth?.accessToken) {
    console.error('[claudeOAuthApi] No OAuth credentials found');
    return errorSnapshot(
      'unauthorized',
      'No OAuth credentials found. Please authenticate with Claude Code CLI first:\n  claude\nThen try again.',
    );
  }

  console.log('[claudeOAuthApi] Credentials found, accessToken length:', credentials.claudeAiOauth.accessToken.length);

  // Step 2: Fetch usage from API
  const apiData = await fetchUsageFromApi(credentials.claudeAiOauth.accessToken);

  if (!apiData) {
    console.error('[claudeOAuthApi] Failed to fetch usage from API');
    return errorSnapshot(
      'unauthorized',
      'Failed to fetch usage from API. Your OAuth token may be expired.\nPlease re-authenticate with Claude Code CLI:\n  claude',
    );
  }

  // Step 3: Validate response structure
  if (!apiData.five_hour || !apiData.seven_day) {
    console.error('[claudeOAuthApi] Invalid API response structure');
    return errorSnapshot('error', 'Invalid API response structure');
  }

  // Step 4: Convert to snapshot
  return apiResponseToSnapshot(apiData);
}

export class ClaudeOAuthApiService {
  async fetchUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
    return fetchOAuthUsageSnapshot();
  }
}
