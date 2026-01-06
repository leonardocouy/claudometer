/**
 * Claude OAuth API Client
 * Fetches usage data directly from Anthropic API using OAuth tokens from ~/.claude/.credentials.json
 * This is the simplest and most reliable method - no CLI, no parsing, just HTTP requests.
 */

import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';

const API_BASE_URL = 'https://api.anthropic.com';
const USAGE_ENDPOINT = '/api/oauth/usage';
const REQUEST_TIMEOUT_MS = 10000;

interface ClaudeCredentials {
  claudeAiOauth?: {
    accessToken: string;
    refreshToken: string;
    expiresAt: number;
    scopes: string[];
    subscriptionType?: string;
  };
  apiKey?: string;
}

interface UsageMetric {
  utilization: number;
  resets_at: string;
}

interface UsageApiResponse {
  five_hour: UsageMetric;
  seven_day: UsageMetric;
  seven_day_opus: UsageMetric;
}

/**
 * Get the Claude credentials file path.
 */
function getCredentialsPath(): string {
  const home = process.env.HOME || process.env.USERPROFILE || '/tmp';
  return path.join(home, '.claude', '.credentials.json');
}

/**
 * Read OAuth token from ~/.claude/.credentials.json
 */
async function readClaudeCredentials(): Promise<
  | { ok: true; token: string; type: 'oauth' | 'apikey' }
  | { ok: false; error: string }
> {
  const credentialsPath = getCredentialsPath();

  if (!existsSync(credentialsPath)) {
    return {
      ok: false,
      error: 'Claude credentials not found. Please run `claude` CLI once to authenticate.',
    };
  }

  try {
    const content = await readFile(credentialsPath, 'utf-8');
    const credentials: ClaudeCredentials = JSON.parse(content);

    // Prefer OAuth token
    if (credentials.claudeAiOauth?.accessToken) {
      return {
        ok: true,
        token: credentials.claudeAiOauth.accessToken,
        type: 'oauth',
      };
    }

    // Fallback to API key
    if (credentials.apiKey) {
      return {
        ok: true,
        token: credentials.apiKey,
        type: 'apikey',
      };
    }

    return {
      ok: false,
      error: 'No valid credentials found in ~/.claude/.credentials.json',
    };
  } catch (err) {
    return {
      ok: false,
      error: `Failed to read credentials: ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}

/**
 * Fetch usage data from the Anthropic API.
 */
async function fetchUsageFromApi(): Promise<
  | { ok: true; data: UsageApiResponse }
  | { ok: false; error: string; status?: number }
> {
  const credentials = await readClaudeCredentials();

  if (!credentials.ok) {
    return { ok: false, error: credentials.error };
  }

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'User-Agent': 'claudometer/1.0',
  };

  // Set auth header based on credential type
  if (credentials.type === 'oauth') {
    headers['Authorization'] = `Bearer ${credentials.token}`;
    headers['anthropic-beta'] = 'oauth-2025-04-20';
  } else {
    headers['x-api-key'] = credentials.token;
  }

  try {
    const response = await fetch(`${API_BASE_URL}${USAGE_ENDPOINT}`, {
      method: 'GET',
      headers,
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });

    if (!response.ok) {
      const errorText = await response.text().catch(() => 'Unknown error');
      return {
        ok: false,
        error: `API request failed (${response.status}): ${errorText}`,
        status: response.status,
      };
    }

    const data = await response.json();
    console.log('[ClaudeOAuthAPI] Raw API response:', JSON.stringify(data, null, 2));

    // Validate response structure
    if (!data || typeof data !== 'object') {
      return {
        ok: false,
        error: 'Invalid API response: not an object',
      };
    }

    if (!data.five_hour || !data.seven_day || !data.seven_day_opus) {
      return {
        ok: false,
        error: `Invalid API response structure. Missing required fields. Got: ${JSON.stringify(data)}`,
      };
    }

    return { ok: true, data };
  } catch (err) {
    if (err instanceof Error) {
      if (err.name === 'AbortError') {
        return { ok: false, error: 'API request timed out' };
      }
      return { ok: false, error: `Network error: ${err.message}` };
    }
    return { ok: false, error: 'Unknown error occurred' };
  }
}

/**
 * Convert API response to ClaudeUsageSnapshot format.
 */
function apiResponseToSnapshot(data: UsageApiResponse): ClaudeUsageSnapshot {
  return {
    status: 'ok',
    organizationId: 'oauth-user', // No organization concept in OAuth API
    sessionPercent: data.five_hour.utilization,
    sessionResetsAt: data.five_hour.resets_at,
    weeklyPercent: data.seven_day.utilization,
    weeklyResetsAt: data.seven_day.resets_at,
    modelWeeklyPercent: data.seven_day_opus.utilization,
    modelWeeklyName: 'Opus',
    modelWeeklyResetsAt: data.seven_day_opus.resets_at,
    lastUpdatedAt: new Date().toISOString(),
  };
}

/**
 * Create error snapshot.
 */
function apiErrorSnapshot(
  status: 'error' | 'unauthorized',
  message: string,
): ClaudeUsageSnapshot {
  return {
    status,
    lastUpdatedAt: new Date().toISOString(),
    errorMessage: message,
  };
}

/**
 * Fetch usage snapshot using API approach.
 */
export async function fetchOAuthUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
  const result = await fetchUsageFromApi();

  if (!result.ok) {
    // Determine if it's an auth error (401/403)
    const isAuthError =
      result.status === 401 ||
      result.status === 403 ||
      result.error.includes('unauthorized') ||
      result.error.includes('credentials not found');

    return apiErrorSnapshot(isAuthError ? 'unauthorized' : 'error', result.error);
  }

  return apiResponseToSnapshot(result.data);
}

export class ClaudeOAuthApiService {
  async fetchUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
    return fetchOAuthUsageSnapshot();
  }

  async testConnection(): Promise<{ ok: true } | { ok: false; error: string }> {
    const credentials = await readClaudeCredentials();
    if (!credentials.ok) {
      return { ok: false, error: credentials.error };
    }
    return { ok: true };
  }
}
