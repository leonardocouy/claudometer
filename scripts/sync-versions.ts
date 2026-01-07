import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

function readJson(path: string): any {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function writeJson(path: string, value: any): void {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function formatJson(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function writeJsonIfChanged(path: string, value: unknown): void {
  const updated = formatJson(value);
  const original = readFileSync(path, 'utf8');
  if (updated !== original) {
    writeFileSync(path, updated, 'utf8');
  }
}

function findCargoPackageSection(contents: string): { start: number; end: number } | null {
  const headerMatch = /(^|\r?\n)\[package\]\s*\r?\n/.exec(contents);
  if (!headerMatch) return null;

  const headerStart = headerMatch.index + headerMatch[1].length;
  const afterHeader = headerStart + headerMatch[0].length - headerMatch[1].length;

  const nextSectionMatch = /\r?\n\[[^\]]+\]/.exec(contents.slice(afterHeader));
  const end = nextSectionMatch ? afterHeader + nextSectionMatch.index + 1 : contents.length;

  return { start: headerStart, end };
}

function readCargoPackageVersion(contents: string): { version: string; indent: string } | null {
  const section = findCargoPackageSection(contents);
  if (!section) return null;

  const sectionText = contents.slice(section.start, section.end);
  const match = /^(\s*)version\s*=\s*"([^"]+)"\s*$/m.exec(sectionText);
  if (!match) return null;

  return { indent: match[1], version: match[2] };
}

function updateCargoTomlVersion(cargoTomlPath: string, version: string, write: boolean): boolean {
  const original = readFileSync(cargoTomlPath, 'utf8');
  const current = readCargoPackageVersion(original);
  if (!current) {
    throw new Error(`Failed to locate [package] version in ${cargoTomlPath}`);
  }

  if (current.version === version) return false;
  if (!write) return true;

  const section = findCargoPackageSection(original);
  if (!section) {
    throw new Error(`Failed to locate [package] section in ${cargoTomlPath}`);
  }

  const sectionText = original.slice(section.start, section.end);
  const updatedSectionText = sectionText.replace(
    /^(\s*)version\s*=\s*"[^"]+"\s*$/m,
    `${current.indent}version = "${version}"`
  );

  const updated = `${original.slice(0, section.start)}${updatedSectionText}${original.slice(section.end)}`;
  writeFileSync(cargoTomlPath, updated, 'utf8');
  return true;
}

function parseArgs(argv: string[]): { check: boolean } {
  return { check: argv.includes('--check') };
}

function main(): void {
  const { check } = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, '..');
  const pkg = readJson(resolve(repoRoot, 'package.json'));
  const version = String(pkg.version || '').trim();
  if (!version) throw new Error('package.json is missing a version');

  const tauriConfPath = resolve(repoRoot, 'src-tauri/tauri.conf.json');
  const tauriConf = readJson(tauriConfPath);
  const tauriConfVersion = String(tauriConf?.version || '').trim();

  const cargoTomlPath = resolve(repoRoot, 'src-tauri/Cargo.toml');
  const cargoContents = readFileSync(cargoTomlPath, 'utf8');
  const cargoVersion = readCargoPackageVersion(cargoContents)?.version ?? '';

  const drift: string[] = [];
  if (tauriConfVersion !== version) {
    drift.push(`src-tauri/tauri.conf.json: "${tauriConfVersion || '(missing)'}"`);
  }
  if (cargoVersion !== version) {
    drift.push(`src-tauri/Cargo.toml: "${cargoVersion || '(missing)'}"`);
  }

  if (check) {
    if (drift.length > 0) {
      console.error(`Version drift detected (package.json is "${version}"):`);
      for (const line of drift) console.error(`- ${line}`);
      console.error('Run `bun run sync-versions` to rewrite versioned files.');
      process.exit(1);
    }
    return;
  }

  tauriConf.version = version;
  writeJsonIfChanged(tauriConfPath, tauriConf);
  updateCargoTomlVersion(cargoTomlPath, version, true);
}

main();
