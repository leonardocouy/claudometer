/**
 * Claude CLI Usage Provider
 * Fetches usage data by running the local `claude /usage` command.
 */

import { spawn } from 'node:child_process';
import { app } from 'electron';
import {
  cliErrorSnapshot,
  cliUsageToSnapshot,
  parseCliUsageOutput,
} from '../../common/cliParser.ts';
import type { ClaudeUsageSnapshot } from '../../common/types.ts';

const DEFAULT_TIMEOUT_MS = 30_000;

export type CliExecutionResult =
  | { ok: true; output: string }
  | { ok: false; error: 'not_found' | 'timeout' | 'execution_error'; message: string };

/**
 * Execute the Claude CLI /usage command.
 * First tries direct spawn, falls back to script PTY wrapper if needed.
 */
export async function executeClaudeUsage(
  cliPath: string,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<CliExecutionResult> {
  // Run from app's userData directory to avoid "trust this folder" prompts
  const userDataPath = app.getPath('userData');

  return new Promise((resolve) => {
    let output = '';
    let resolved = false;

    const safeResolve = (result: CliExecutionResult) => {
      if (resolved) return;
      resolved = true;
      resolve(result);
    };

    // Try direct spawn first - simpler and may work if no TTY needed for /usage
    const child = spawn(cliPath, ['/usage'], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, TERM: 'xterm-256color', FORCE_COLOR: '1' },
      timeout: timeoutMs,
      cwd: userDataPath,
    });

    const timeoutId = setTimeout(() => {
      child.kill('SIGTERM');
      setTimeout(() => {
        if (!child.killed) child.kill('SIGKILL');
      }, 1000);
      safeResolve({
        ok: false,
        error: 'timeout',
        message: 'Claude CLI timed out.',
      });
    }, timeoutMs);

    child.stdout?.on('data', (data: Buffer) => {
      output += data.toString();
    });

    child.stderr?.on('data', (data: Buffer) => {
      output += data.toString();
    });

    child.on('error', (err) => {
      clearTimeout(timeoutId);
      const message =
        err.message.includes('ENOENT') || err.message.includes('not found')
          ? 'Claude CLI not found. Configure the path in Settings.'
          : `Claude CLI execution error: ${err.message}`;
      safeResolve({
        ok: false,
        error: err.message.includes('ENOENT') ? 'not_found' : 'execution_error',
        message,
      });
    });

    child.on('close', (code) => {
      clearTimeout(timeoutId);

      // Send escape key to exit the /usage view (if still waiting)
      child.stdin?.write('\x1b');
      child.stdin?.end();

      // Check if we got useful output even with non-zero exit
      if (output.includes('Current session') || output.includes('Error:')) {
        safeResolve({ ok: true, output });
        return;
      }

      if (code !== 0 && !output.trim()) {
        safeResolve({
          ok: false,
          error: 'execution_error',
          message: `Claude CLI exited with code ${code}.`,
        });
        return;
      }

      safeResolve({ ok: true, output });
    });

    // Send escape after a short delay to exit the /usage view
    setTimeout(() => {
      if (!resolved) {
        child.stdin?.write('\x1b'); // ESC to exit
      }
    }, 5000); // Give it 5 seconds to render
  });
}

/**
 * Fetch usage snapshot from Claude CLI.
 */
export async function fetchCliUsageSnapshot(cliPath: string): Promise<ClaudeUsageSnapshot> {
  const result = await executeClaudeUsage(cliPath);

  if (!result.ok) {
    return cliErrorSnapshot('error', result.message);
  }

  // Check for trust prompt - user needs to approve the directory first
  if (result.output.includes('Do you trust the files in this folder?')) {
    const userDataPath = app.getPath('userData');
    return cliErrorSnapshot(
      'error',
      `Claude Code requires folder approval. Please run once:\ncd "${userDataPath}" && claude\nThen accept the trust prompt and exit (Ctrl+C).`,
    );
  }

  const parsed = parseCliUsageOutput(result.output);

  if (!parsed.ok) {
    const status = parsed.error === 'unauthorized' ? 'unauthorized' : 'error';
    return cliErrorSnapshot(status, parsed.message);
  }

  return cliUsageToSnapshot(parsed.data);
}

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
    return fetchCliUsageSnapshot(this.cliPath);
  }
}
