# Releasing

How to release a new version of Cmdr.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub secrets
- Release notes under `## [Unreleased]` in `CHANGELOG.md` (add as you develop, commit before releasing)
- Clean working tree (script auto-fails if not satisfied)

## Release steps

1. Run `./scripts/release.sh 1.2.1` – version bump guidelines:
   - Patch (1.2.0 → 1.2.1): bug fixes, minor tweaks
   - Minor (1.2.1 → 1.3.0): new features
   - Major (1.3.0 → 2.0.0): major launches
2. Script bumps version in `package.json`, `tauri.conf.json`, `Cargo.toml`
3. Script moves `[Unreleased]` → `[1.2.1]` in changelog, commits, and tags
4. Push: `git push origin main --tags`
5. CI builds universal macOS binary, creates GitHub release, updates `latest.json`
6. Website auto-deploys (~8 min), users get update notification on next check

## How updates work

- App checks `https://getcmdr.com/latest.json` on start and every 60 min
- If newer version found → downloads silently → shows "Restart to update" toast
- Signatures verified with public key embedded in app
