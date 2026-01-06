/**
 * Claude OAuth API Client
 * Fetches usage data from Anthropic's OAuth API using credentials from ~/.claude/.credentials.json
 */

import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { z } from 'zod';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';
import { nowIso } from '../../common/types.ts';
import { fetchWithTimeout } from '../../common/fetch-with-timeout.ts';
import { sanitizeError } from '../../common/sanitization.ts';
import { parseUtilizationPercent } from '../../common/parser.ts';

/**
 * Error thrown when OAuth API requests fail
 * Includes proper status mapping to distinguish between unauthorized, rate_limited, and transient errors
 */
class ClaudeOAuthRequestError extends Error {
  status: 'unauthorized' | 'rate_limited' | 'error';
  httpStatus?: number;

  constructor(
    message: string,
    status: 'unauthorized' | 'rate_limited' | 'error',
    httpStatus?: number,
  ) {
    super(message);
    this.name = 'ClaudeOAuthRequestError';
    this.status = status;
    this.httpStatus = httpStatus;
  }
}

/**
 * Map HTTP status codes to usage status
 * - 401, 403: unauthorized (stop polling, require re-auth)
 * - 429: rate_limited (backoff and retry)
 * - 5xx, network errors: error (transient, continue polling)
 */
function mapHttpStatusToUsageStatus(httpStatus: number): 'unauthorized' | 'rate_limited' | 'error' {
  if (httpStatus === 401 || httpStatus === 403) {
    return 'unauthorized';
  }
  if (httpStatus === 429) {
    return 'rate_limited';
  }
  // 5xx errors and others are transient
  return 'error';
}

/**
 * Zod schema for OAuth credentials file (~/.claude/.credentials.json)
 */
const OAuthCredentialsSchema = z.object({
  claudeAiOauth: z
    .object({
      accessToken: z.string().min(1),
      refreshToken: z.string().min(1),
      expiresAt: z.number(),
    })
    .optional(),
  apiKey: z.string().optional(),
});

/**
 * Zod schema for usage bucket in API response
 * Note: API returns utilization as integer 0-100, not decimal 0-1
 */
const UsageBucketSchema = z.object({
  utilization: z.number().min(0).max(100),
  resets_at: z.string().optional(),
});

/**
 * Zod schema for OAuth API usage response
 */
const UsageApiResponseSchema = z.object({
  five_hour: UsageBucketSchema.optional(),
  seven_day: UsageBucketSchema.optional(),
  seven_day_opus: UsageBucketSchema.nullable().optional(),
  seven_day_sonnet: UsageBucketSchema.nullable().optional(),
});

// TypeScript types inferred from Zod schemas
type OAuthCredentials = z.infer<typeof OAuthCredentialsSchema>;
type UsageApiResponse = z.infer<typeof UsageApiResponseSchema>;

function errorSnapshot(status: 'error' | 'unauthorized' | 'rate_limited', message: string): ClaudeUsageSnapshot {
  return {
    status,
    lastUpdatedAt: nowIso(),
    errorMessage: message,
  };
}

/**
 * Read OAuth credentials from ~/.claude/.credentials.json
 * Validates the structure using Zod schema
 */
async function readCredentials(): Promise<OAuthCredentials | null> {
  try {
    const credentialsPath = join(homedir(), '.claude', '.credentials.json');
    const content = await readFile(credentialsPath, 'utf-8');
    const parsed = JSON.parse(content);

    // Validate with Zod schema
    const validationResult = OAuthCredentialsSchema.safeParse(parsed);
    if (!validationResult.success) {
      console.error('[claudeOAuthApi] Invalid credentials file structure:', validationResult.error.format());
      return null;
    }

    return validationResult.data;
  } catch (error) {
    console.error('[claudeOAuthApi] Failed to read credentials:', sanitizeError(error));
    return null;
  }
}

/**
 * Validate OAuth credentials for pre-save checks
 * Returns { valid: true } if credentials exist and are valid
 * Returns { valid: false, error: string } if validation fails
 */
export async function validateOAuthCredentials(): Promise<
  { valid: true } | { valid: false; error: string }
> {
  const credentials = await readCredentials();

  if (!credentials) {
    return {
      valid: false,
      error: 'Could not read OAuth credentials file (~/.claude/.credentials.json).',
    };
  }

  if (!credentials.claudeAiOauth?.accessToken) {
    return {
      valid: false,
      error: 'No OAuth credentials found. Please authenticate with Claude Code CLI first:\n  claude',
    };
  }

  return { valid: true };
}

/**
 * Fetch usage data from Anthropic OAuth API
 * @throws ClaudeOAuthRequestError with appropriate status (unauthorized, rate_limited, or error)
 */
async function fetchUsageFromApi(accessToken: string): Promise<UsageApiResponse> {
  try {
    const response = await fetchWithTimeout('https://api.anthropic.com/api/oauth/usage', {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${accessToken}`,
        'anthropic-beta': 'oauth-2025-04-20',
      },
    });

    if (!response.ok) {
      const status = mapHttpStatusToUsageStatus(response.status);
      const message = `OAuth API request failed (HTTP ${response.status})`;
      console.error(`[claudeOAuthApi] ${message}:`, status);
      throw new ClaudeOAuthRequestError(message, status, response.status);
    }

    // Parse and validate response with Zod schema
    const jsonData = await response.json();
    const validationResult = UsageApiResponseSchema.safeParse(jsonData);

    if (!validationResult.success) {
      console.error('[claudeOAuthApi] Invalid API response structure:', validationResult.error.format());
      throw new ClaudeOAuthRequestError(
        'Invalid API response structure',
        'error',
      );
    }

    return validationResult.data;
  } catch (error) {
    // If already a ClaudeOAuthRequestError, re-throw it
    if (error instanceof ClaudeOAuthRequestError) {
      throw error;
    }
    // Network errors, timeout errors, etc. are transient -> status: 'error'
    console.error('[claudeOAuthApi] Network error:', sanitizeError(error));
    throw new ClaudeOAuthRequestError(
      'Network error during OAuth API request',
      'error',
    );
  }
}

/**
 * Convert API response to ClaudeUsageSnapshot
 */
function apiResponseToSnapshot(data: UsageApiResponse): ClaudeUsageSnapshot {
  // Session (5-hour) usage
  // Use shared parseUtilizationPercent for consistent clamping/rounding across modes
  const sessionPercent = parseUtilizationPercent(data.five_hour?.utilization);
  const sessionResetsAt = data.five_hour?.resets_at;

  // Weekly (7-day) usage
  const weeklyPercent = parseUtilizationPercent(data.seven_day?.utilization);
  const weeklyResetsAt = data.seven_day?.resets_at;

  // Model-specific weekly usage (fallback: Opus → Sonnet → 0)
  // TODO: Task 2.5 (DEFERRED) - Support multiple models instead of picking one
  let modelWeeklyPercent = 0;
  let modelWeeklyName: string | undefined;
  let modelWeeklyResetsAt: string | undefined;

  if (data.seven_day_opus) {
    modelWeeklyPercent = parseUtilizationPercent(data.seven_day_opus.utilization);
    modelWeeklyName = 'Opus';
    modelWeeklyResetsAt = data.seven_day_opus.resets_at;
  } else if (data.seven_day_sonnet) {
    modelWeeklyPercent = parseUtilizationPercent(data.seven_day_sonnet.utilization);
    modelWeeklyName = 'Sonnet';
    modelWeeklyResetsAt = data.seven_day_sonnet.resets_at;
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

  return snapshot;
}

/**
 * Fetch usage snapshot using OAuth credentials
 */
export async function fetchOAuthUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
  try {
    // Step 1: Read credentials
    const credentials = await readCredentials();

    if (!credentials?.claudeAiOauth?.accessToken) {
      console.error('[claudeOAuthApi] No OAuth credentials found');
      return errorSnapshot(
        'unauthorized',
        'No OAuth credentials found. Please authenticate with Claude Code CLI first:\n  claude\nThen try again.',
      );
    }

    // Step 2: Fetch usage from API (may throw ClaudeOAuthRequestError)
    const apiData = await fetchUsageFromApi(credentials.claudeAiOauth.accessToken);

    // Step 3: Validate response structure
    if (!apiData.five_hour || !apiData.seven_day) {
      console.error('[claudeOAuthApi] Invalid API response structure');
      return errorSnapshot('error', 'Invalid API response structure');
    }

    // Step 4: Convert to snapshot
    return apiResponseToSnapshot(apiData);
  } catch (error) {
    // Handle ClaudeOAuthRequestError with proper status propagation
    if (error instanceof ClaudeOAuthRequestError) {
      const errorMessage =
        error.status === 'unauthorized'
          ? 'Authentication failed. Your OAuth token may be expired.\nPlease re-authenticate with Claude Code CLI:\n  claude'
          : error.status === 'rate_limited'
            ? 'Rate limit exceeded. Please wait a moment and try again.'
            : 'Temporary error accessing Claude API. Will retry automatically.';
      return errorSnapshot(error.status, errorMessage);
    }
    // Unknown errors are treated as transient
    console.error('[claudeOAuthApi] Unexpected error:', sanitizeError(error));
    return errorSnapshot('error', 'Unexpected error occurred');
  }
}

// ClaudeOAuthApiService class removed - use fetchOAuthUsageSnapshot() directly
