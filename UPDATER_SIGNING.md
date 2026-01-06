# Updater signing (Tauri v2)

Claudometer uses `tauri-plugin-updater` with a `latest.json` manifest hosted on GitHub Releases:

`https://github.com/<owner>/<repo>/releases/latest/download/latest.json`

Update artifacts are verified using a **public signing key** committed in `src-tauri/tauri.conf.json`, and signatures generated from a **private signing key** stored in GitHub Actions secrets.

## 1) Generate a signer keypair (local)

Pick a safe location outside the repo (do not commit the private key):

```bash
./scripts/generate-updater-keypair.sh "$HOME/.config/claudometer/tauri-updater.key"
```

If you prefer keeping a local copy in the repo for convenience, use `.secrets/` (it is gitignored):

```bash
./scripts/generate-updater-keypair.sh "$PWD/.secrets/tauri-updater.key"
```

**Warning**: `.gitignore` reduces accidental commits, but it does not eliminate leakage risk. Treat the private key as a long-lived release credential.

This writes:
- private key: `.../tauri-updater.key` (keep secret)
- public key: `.../tauri-updater.key.pub`

## 2) Configure the public key in the app

Copy the contents of the `.pub` file into:

- `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`

## 3) Configure GitHub Actions secrets

Add repository secrets:
- `TAURI_SIGNING_PRIVATE_KEY`: **contents** of the private key file (not the path)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password you chose (can be empty)

The CI build uses these to generate `.sig` files for updater payloads.

## 4) `latest.json` generation

CI generates `latest.json` in the Tauri “static manifest” format:

```json
{
  "version": "vX.Y.Z",
  "notes": "https://github.com/<owner>/<repo>/releases/tag/vX.Y.Z",
  "pub_date": "2026-01-01T00:00:00Z",
  "platforms": {
    "linux-x86_64": { "url": "...", "signature": "..." },
    "darwin-aarch64": { "url": "...", "signature": "..." }
  }
}
```

The helper script used in CI is `scripts/generate-latest-json.ts`.

## Smoke test (recommended)

1. Ensure `plugins.updater.pubkey` is set in `src-tauri/tauri.conf.json` and CI secrets are configured.
2. Merge a Release Please version bump so a GitHub Release is created.
3. Confirm the release contains:
   - platform artifacts (macOS + Linux)
   - corresponding `.sig` files
   - `latest.json`
4. Install the previous version and use tray → “Check for Updates…”.

## Key rotation warning

If you rotate the updater signing key, existing installs will not be able to verify updates signed with a different key. Treat the private key and password as long-lived release credentials.
