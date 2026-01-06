import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

type PlatformInput = {
  key: string;
  artifactPath: string;
  signaturePath: string;
};

const BASE64_RE = /^[A-Za-z0-9+/=]+$/;

function usageAndExit(message?: string): never {
  if (message) {
    // eslint-disable-next-line no-console
    console.error(message);
  }
  // eslint-disable-next-line no-console
  console.error(
    [
      'Usage:',
      '  bun run scripts/generate-latest-json.ts \\',
      '    --repo owner/repo \\',
      '    --tag vX.Y.Z \\',
      '    --platform <key>:<artifactPath>:<sigPath> \\',
      '    [--platform ...] \\',
      '    [--notes <url-or-text>] \\',
      '    [--pub-date <iso>] \\',
      '    [--out latest.json]',
      '',
      'Example:',
      '  bun run scripts/generate-latest-json.ts \\',
      '    --repo leonardocouy/claudometer \\',
      '    --tag v1.3.1 \\',
      '    --platform linux-x86_64:dist/Claudometer.AppImage:dist/Claudometer.AppImage.sig \\',
      '    --platform darwin-aarch64:dist/Claudometer.app.tar.gz:dist/Claudometer.app.tar.gz.sig',
    ].join('\n'),
  );
  process.exit(2);
}

function parseArgs(argv: string[]) {
  const repo = getValue(argv, '--repo');
  const tag = getValue(argv, '--tag');
  const notes = getValue(argv, '--notes');
  const pubDate = getValue(argv, '--pub-date');
  const out = getValue(argv, '--out') ?? 'latest.json';
  const platforms = getMultiValue(argv, '--platform');

  if (!repo) usageAndExit('Missing --repo');
  if (!tag) usageAndExit('Missing --tag');
  if (platforms.length === 0) usageAndExit('Missing --platform');

  const parsedPlatforms: PlatformInput[] = platforms.map((raw) => {
    const parts = raw.split(':');
    if (parts.length < 3) {
      usageAndExit(`Invalid --platform value: ${raw}`);
    }
    const key = parts[0]!.trim();
    const artifactPath = parts[1]!.trim();
    const signaturePath = parts.slice(2).join(':').trim();
    if (!key || !artifactPath || !signaturePath) {
      usageAndExit(`Invalid --platform value: ${raw}`);
    }
    return { key, artifactPath, signaturePath };
  });

  return { repo, tag, notes, pubDate, out, platforms: parsedPlatforms };
}

function getValue(argv: string[], flag: string): string | null {
  const idx = argv.indexOf(flag);
  if (idx === -1) return null;
  const value = argv[idx + 1];
  if (!value || value.startsWith('--')) return null;
  return value;
}

function getMultiValue(argv: string[], flag: string): string[] {
  const out: string[] = [];
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] !== flag) continue;
    const value = argv[i + 1];
    if (!value || value.startsWith('--')) usageAndExit(`Missing value for ${flag}`);
    out.push(value);
    i += 1;
  }
  return out;
}

async function readSignatureBase64(signaturePath: string): Promise<string> {
  const raw = (await readFile(signaturePath, 'utf8')).trim();
  if (raw.length === 0) {
    throw new Error(`Empty signature file: ${signaturePath}`);
  }
  if (BASE64_RE.test(raw)) {
    return raw;
  }
  return Buffer.from(raw, 'utf8').toString('base64');
}

async function main() {
  const { repo, tag, notes, pubDate, out, platforms } = parseArgs(process.argv.slice(2));

  const downloadBase = `https://github.com/${repo}/releases/download/${tag}/`;
  const manifest = {
    version: tag,
    notes: notes ?? `https://github.com/${repo}/releases/tag/${tag}`,
    pub_date: pubDate ?? new Date().toISOString(),
    platforms: {} as Record<string, { url: string; signature: string }>,
  };

  for (const platform of platforms) {
    const filename = path.basename(platform.artifactPath);
    if (!filename) {
      throw new Error(`Invalid artifact path: ${platform.artifactPath}`);
    }
    const signature = await readSignatureBase64(platform.signaturePath);
    manifest.platforms[platform.key] = {
      url: `${downloadBase}${encodeURIComponent(filename)}`,
      signature,
    };
  }

  await writeFile(out, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
}

await main();

