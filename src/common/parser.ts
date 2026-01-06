import type { ClaudeUsageSnapshot } from './types.ts';

type JsonValue = null | boolean | number | string | JsonObject | JsonValue[];
type JsonObject = { [key: string]: JsonValue };

function clampPercent(value: number): number {
  if (Number.isNaN(value)) return 0;
  return Math.max(0, Math.min(100, value));
}

export function parseUtilizationPercent(value: unknown): number {
  if (typeof value === 'number') return clampPercent(value);
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return 0;
    const parsed = Number(trimmed);
    if (Number.isFinite(parsed)) return clampPercent(parsed);
    return 0;
  }
  return 0;
}

function readObject(value: unknown): JsonObject | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  return value as JsonObject;
}

function readString(value: unknown): string | undefined {
  if (typeof value === 'string' && value.trim()) return value;
  return undefined;
}

export function parseClaudeUsageFromJson(
  json: unknown,
  organizationId: string,
  lastUpdatedAt: string,
): Extract<ClaudeUsageSnapshot, { status: 'ok' }> {
  const root = readObject(json) ?? {};

  const fiveHour = readObject(root.five_hour);
  const sevenDay = readObject(root.seven_day);

  const sessionPercent = parseUtilizationPercent(fiveHour?.utilization);
  const sessionResetsAt = readString(fiveHour?.resets_at);

  const weeklyPercent = parseUtilizationPercent(sevenDay?.utilization);
  const weeklyResetsAt = readString(sevenDay?.resets_at);

  const models = readModelWeeklyUsages(root);

  return {
    status: 'ok',
    organizationId,
    sessionPercent,
    sessionResetsAt,
    weeklyPercent,
    weeklyResetsAt,
    models,
    lastUpdatedAt,
  };
}

export function mapHttpStatusToUsageStatus(
  statusCode: number,
): 'unauthorized' | 'rate_limited' | 'error' {
  if (statusCode === 401 || statusCode === 403) return 'unauthorized';
  if (statusCode === 429) return 'rate_limited';
  return 'error';
}

function titleCase(value: string): string {
  return value
    .split(/[_\s]+/g)
    .filter(Boolean)
    .map((part) => part[0]?.toUpperCase() + part.slice(1))
    .join(' ');
}

function readModelWeeklyUsages(
  root: JsonObject,
): { name: string; percent: number; resetsAt?: string }[] {
  // The web API has historically returned `seven_day_opus`, but some accounts appear to get
  // `seven_day_sonnet` (and potentially other `seven_day_*` keys). Prefer Sonnet if present.
  const preferredKeys = ['seven_day_sonnet', 'seven_day_opus'];

  const out: { name: string; percent: number; resetsAt?: string }[] = [];
  for (const key of preferredKeys) {
    const period = readObject(root[key]);
    if (!period) continue;
    out.push({
      percent: parseUtilizationPercent(period.utilization),
      name: titleCase(key.replace('seven_day_', '')),
      resetsAt: readString(period.resets_at),
    });
  }

  // Then add any other seven_day_* models not already seen
  for (const [key, value] of Object.entries(root)) {
    if (!key.startsWith('seven_day_')) continue;
    if (key === 'seven_day') continue;
    if (preferredKeys.includes(key)) continue;
    const period = readObject(value);
    if (!period) continue;
    const percent = parseUtilizationPercent(period.utilization);
    if (percent === 0) continue;
    out.push({
      percent,
      name: titleCase(key.replace('seven_day_', '')),
      resetsAt: readString(period.resets_at),
    });
  }

  return out;
}
