import fs from 'node:fs';

export function tryTerminalBell(): boolean {
  let ok = false;

  try {
    // Works when the Electron main process has a controlling terminal.
    if (process.stdout && typeof process.stdout.write === 'function') {
      ok = process.stdout.write('\x07') || ok;
    }
    if (process.stderr && typeof process.stderr.write === 'function') {
      ok = process.stderr.write('\x07') || ok;
    }
  } catch {
    // ignore
  }

  try {
    // Fallback to writing directly to the controlling tty (best-effort).
    const fd = fs.openSync('/dev/tty', 'w');
    try {
      fs.writeSync(fd, '\x07');
      ok = true;
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    // ignore
  }

  return ok;
}
