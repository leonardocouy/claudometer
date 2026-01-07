# Development

## Version sync

`package.json` is the version source of truth for this repo. Keep the following files in sync:

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`

After changing `package.json` version, run:

- `bun run sync-versions` to rewrite the Tauri/Rust version files
- `bun run sync-versions:check` to verify there is no drift (no writes; non-zero exit code on mismatch)

