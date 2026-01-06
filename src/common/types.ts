export type UsageStatus = 'ok' | 'unauthorized' | 'rate_limited' | 'error' | 'missing_key';

export type UsageSource = 'web' | 'cli';

export type ModelUsage = {
  name: string;
  percent: number;
  resetsAt?: string;
};

export type ClaudeUsageSnapshot =
  | {
      status: 'ok';
      organizationId: string;
      sessionPercent: number;
      sessionResetsAt?: string;
      weeklyPercent: number;
      weeklyResetsAt?: string;
      models: ModelUsage[];
      lastUpdatedAt: string;
    }
  | {
      status: Exclude<UsageStatus, 'ok'>;
      organizationId?: string;
      lastUpdatedAt: string;
      errorMessage?: string;
    };

export type ClaudeOrganization = { id: string; name?: string };

export function nowIso(): string {
  return new Date().toISOString();
}
