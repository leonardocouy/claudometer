import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

function readJson(path: string): any {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function writeJson(path: string, value: any): void {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function updateCargoTomlVersion(cargoTomlPath: string, version: string): void {
  const original = readFileSync(cargoTomlPath, 'utf8');
  const re = /^version\s*=\s*"[0-9]+(?:\.[0-9]+){2}(?:[-+][^"]+)?"\s*$/m;
  if (!re.test(original)) {
    throw new Error(`Failed to locate package version in ${cargoTomlPath}`);
  }

  const updated = original.replace(re, `version = "${version}"`);
  if (updated !== original) {
    writeFileSync(cargoTomlPath, updated, 'utf8');
  }
}

function main(): void {
  const repoRoot = resolve(import.meta.dir, '..');
  const pkg = readJson(resolve(repoRoot, 'package.json'));
  const version = String(pkg.version || '').trim();
  if (!version) throw new Error('package.json is missing a version');

  const tauriConfPath = resolve(repoRoot, 'src-tauri/tauri.conf.json');
  const tauriConf = readJson(tauriConfPath);
  tauriConf.version = version;
  writeJson(tauriConfPath, tauriConf);

  updateCargoTomlVersion(resolve(repoRoot, 'src-tauri/Cargo.toml'), version);
}

main();
