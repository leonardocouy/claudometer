/**
 * Claude CLI Usage Provider
 * Fetches usage data from Anthropic's OAuth API using credentials from ~/.claude/.credentials.json
 */

import type { ClaudeUsageSnapshot } from '../../common/types.ts';
import { fetchOAuthUsageSnapshot } from './claudeOAuthApi.ts';

export class ClaudeCliService {
  private cliPath: string;

  constructor(cliPath = 'claude') {
    this.cliPath = cliPath;
  }

  setCliPath(path: string): void {
    this.cliPath = path.trim() || 'claude';
  }

  getCliPath(): string {
    return this.cliPath;
  }

  async fetchUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
    return fetchOAuthUsageSnapshot();
  }
}
