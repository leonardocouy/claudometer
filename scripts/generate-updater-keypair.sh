#!/usr/bin/env bash
set -euo pipefail

OUT_PATH="${1:-}"
if [[ -z "${OUT_PATH}" ]]; then
  echo "Usage: scripts/generate-updater-keypair.sh <output-private-key-path>"
  echo "Example: scripts/generate-updater-keypair.sh \"$HOME/.config/claudometer/tauri-updater.key\""
  exit 2
fi

PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
if [[ -z "${PASSWORD}" ]]; then
  read -r -s -p "Private key password (leave empty for none): " PASSWORD
  echo ""
fi

ARGS=(bunx tauri signer generate --ci --write-keys "${OUT_PATH}")
if [[ -n "${PASSWORD}" ]]; then
  ARGS+=(--password "${PASSWORD}")
fi

"${ARGS[@]}"

echo ""
echo "Public key (paste into src-tauri/tauri.conf.json -> plugins.updater.pubkey):"
cat "${OUT_PATH}.pub"
echo ""
echo "GitHub Secrets:"
echo "  TAURI_SIGNING_PRIVATE_KEY: (contents of ${OUT_PATH})"
echo "  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: (the password you chose; can be empty)"

