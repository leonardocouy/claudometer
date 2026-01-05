import { type ClaudeUsageSnapshot, nowIso } from '../shared/claudeUsage.ts';
import { mapHttpStatusToUsageStatus, parseClaudeUsageFromJson } from '../shared/usageParser.ts';

export type ClaudeOrganization = { id: string; name?: string };

const DEBUG_CLAUDE_WEB = process.env.CLAUDE_USAGE_DEBUG === '1';

function debugLog(...args: unknown[]): void {
  if (!DEBUG_CLAUDE_WEB) return;
  // eslint-disable-next-line no-console
  console.log('[claude-web]', ...args);
}

class ClaudeWebRequestError extends Error {
  status: 'unauthorized' | 'rate_limited' | 'error';
  httpStatus: number;

  constructor(
    message: string,
    status: 'unauthorized' | 'rate_limited' | 'error',
    httpStatus: number,
  ) {
    super(message);
    this.name = 'ClaudeWebRequestError';
    this.status = status;
    this.httpStatus = httpStatus;
  }
}

function sanitizeErrorMessage(message: string): string {
  return message.replaceAll(/sessionKey=[^;\s]+/gi, 'sessionKey=REDACTED');
}

function redactBodyForLogs(body: string): string {
  let redacted = body.replaceAll(/sessionKey=[^;\s]+/gi, 'sessionKey=REDACTED');
  redacted = redacted.replaceAll(/sk-ant-sid01-[A-Za-z0-9_-]+/g, 'sk-ant-sid01-REDACTED');
  return redacted;
}

function buildHeaders(sessionKey: string): HeadersInit {
  return {
    Accept: 'application/json',
    Cookie: `sessionKey=${sessionKey}`,
    'User-Agent':
      'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36',
    Origin: 'https://claude.ai',
    Referer: 'https://claude.ai/',
  };
}

export class ClaudeWebUsageClient {
  async fetchOrganizations(sessionKey: string): Promise<ClaudeOrganization[]> {
    const response = await fetch('https://claude.ai/api/organizations', {
      method: 'GET',
      headers: buildHeaders(sessionKey),
    });

    if (!response.ok) {
      const status = mapHttpStatusToUsageStatus(response.status);
      if (DEBUG_CLAUDE_WEB) {
        const body = redactBodyForLogs(await response.text().catch(() => ''));
        debugLog('organizations response', {
          httpStatus: response.status,
          status,
          contentType: response.headers.get('content-type'),
          bodyPreview: body.slice(0, 600),
        });
      }
      throw new ClaudeWebRequestError(
        `Failed to fetch organizations (${status})`,
        status,
        response.status,
      );
    }

    const json = (await response.json()) as unknown;
    if (!Array.isArray(json)) return [];

    return json
      .map((value) => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
        const record = value as Record<string, unknown>;
        const id = typeof record.uuid === 'string' ? record.uuid : undefined;
        const name = typeof record.name === 'string' ? record.name : undefined;
        if (!id) return null;
        return name
          ? ({ id, name } satisfies ClaudeOrganization)
          : ({ id } satisfies ClaudeOrganization);
      })
      .filter((value): value is ClaudeOrganization => value !== null);
  }

  async fetchUsageSnapshot(
    sessionKey: string,
    organizationId: string,
  ): Promise<ClaudeUsageSnapshot> {
    const lastUpdatedAt = nowIso();
    const url = `https://claude.ai/api/organizations/${encodeURIComponent(organizationId)}/usage`;

    try {
      const response = await fetch(url, {
        method: 'GET',
        headers: buildHeaders(sessionKey),
      });

      if (!response.ok) {
        const status = mapHttpStatusToUsageStatus(response.status);
        if (DEBUG_CLAUDE_WEB) {
          const body = redactBodyForLogs(await response.text().catch(() => ''));
          debugLog('usage response', {
            httpStatus: response.status,
            status,
            organizationId,
            contentType: response.headers.get('content-type'),
            bodyPreview: body.slice(0, 600),
          });
        }
        return {
          status,
          organizationId,
          lastUpdatedAt,
          errorMessage: `Claude API error (${response.status})`,
        };
      }

      const text = await response.text();
      if (DEBUG_CLAUDE_WEB) {
        const redacted = redactBodyForLogs(text);
        const parsed = (() => {
          try {
            return JSON.parse(text) as unknown;
          } catch {
            return null;
          }
        })();
        const root: Record<string, unknown> | null =
          parsed && typeof parsed === 'object' && !Array.isArray(parsed)
            ? (parsed as Record<string, unknown>)
            : null;
        const keys = root ? Object.keys(root) : [];
        const sevenDayKeys = keys.filter((k) => k.startsWith('seven_day_'));
        debugLog('usage json keys', {
          organizationId,
          keys,
          sevenDayKeys,
          bodyPreview: redacted.slice(0, 600),
        });
      }

      const json = JSON.parse(text) as unknown;
      return parseClaudeUsageFromJson(json, organizationId, lastUpdatedAt);
    } catch (error) {
      if (error instanceof ClaudeWebRequestError) {
        return {
          status: error.status,
          organizationId,
          lastUpdatedAt,
          errorMessage: `Claude API error (${error.httpStatus})`,
        };
      }
      const message =
        error instanceof Error ? sanitizeErrorMessage(error.message) : 'Unknown error';
      return { status: 'error', organizationId, lastUpdatedAt, errorMessage: message };
    }
  }
}
