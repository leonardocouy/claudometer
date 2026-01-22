export type UsageStatus = 'ok' | 'unauthorized' | 'rate_limited' | 'error' | 'missing_key';

export type UsageSource = 'web' | 'cli'; // Claude source

export type CodexUsageSource = 'auto' | 'oauth' | 'web' | 'cli';

export type ClaudeModelUsage = {
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
      models: ClaudeModelUsage[];
      lastUpdatedAt: string;
    }
  | {
      status: Exclude<UsageStatus, 'ok'>;
      organizationId?: string;
      lastUpdatedAt: string;
      errorMessage?: string;
    };

export type CodexUsageSnapshot =
  | {
      status: 'ok';
      sessionPercent: number;
      sessionResetsAt?: string;
      weeklyPercent: number;
      weeklyResetsAt?: string;
      lastUpdatedAt: string;
    }
  | {
      status: Exclude<UsageStatus, 'ok'>;
      lastUpdatedAt: string;
      errorMessage?: string;
    };

export type UsageSnapshotBundle = {
  claude: ClaudeUsageSnapshot | null;
  codex: CodexUsageSnapshot | null;
};

export type ClaudeOrganization = { id: string; name?: string };

export function nowIso(): string {
  return new Date().toISOString();
}
