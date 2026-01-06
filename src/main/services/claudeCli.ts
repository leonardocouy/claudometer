/**
 * Claude CLI Usage Provider
 * Fetches usage data from Anthropic's OAuth API using credentials from ~/.claude/.credentials.json
 */

import type { ClaudeUsageSnapshot } from '../../common/types.ts';
import { fetchOAuthUsageSnapshot } from './claudeOAuthApi.ts';

export class ClaudeCliService {
  async fetchUsageSnapshot(): Promise<ClaudeUsageSnapshot> {
    return fetchOAuthUsageSnapshot();
  }
}
