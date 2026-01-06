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
    utilization: number;
    resets_at?: string;
  };
  seven_day?: {
    utilization: number;
    resets_at?: string;
  };
  seven_day_opus?: {
    utilization: number;
    resets_at?: string;
  } | null;
  seven_day_sonnet?: {
    utilization: number;
    resets_at?: string;
  } | null;
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

  // Session (5-hour) usage - API returns utilization as percentage directly
  const sessionPercent = Math.round(data.five_hour?.utilization ?? 0);
  const sessionResetsAt = data.five_hour?.resets_at;
  console.log('[claudeOAuthApi] Session:', {
    utilization: data.five_hour?.utilization,
    percent: sessionPercent
  });

  // Weekly (7-day) usage - API returns utilization as percentage directly
  const weeklyPercent = Math.round(data.seven_day?.utilization ?? 0);
  const weeklyResetsAt = data.seven_day?.resets_at;
  console.log('[claudeOAuthApi] Weekly:', {
    utilization: data.seven_day?.utilization,
    percent: weeklyPercent
  });

  // Model-specific weekly usage (fallback: Opus → Sonnet → 0)
  let modelWeeklyPercent = 0;
  let modelWeeklyName: string | undefined;
  let modelWeeklyResetsAt: string | undefined;

  if (data.seven_day_opus) {
    modelWeeklyPercent = Math.round(data.seven_day_opus.utilization ?? 0);
    modelWeeklyName = 'Opus';
    modelWeeklyResetsAt = data.seven_day_opus.resets_at;
    console.log('[claudeOAuthApi] Model (Opus):', {
      utilization: data.seven_day_opus.utilization,
      percent: modelWeeklyPercent
    });
  } else if (data.seven_day_sonnet) {
    modelWeeklyPercent = Math.round(data.seven_day_sonnet.utilization ?? 0);
    modelWeeklyName = 'Sonnet';
    modelWeeklyResetsAt = data.seven_day_sonnet.resets_at;
    console.log('[claudeOAuthApi] Model (Sonnet):', {
      utilization: data.seven_day_sonnet.utilization,
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
