# Releasing

How to release a new version of Cmdr.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub secrets

## Release steps

1. Ask an agent to update the changelog with this prompt:
   `Read @AGENTS.md and @docs/style-guide.md, then update CHANGELOG.md based on git commits since last release.
   Read the whole changelog to match its style. Note: commits have title + body, read both.`
2. Commit the changelog update and anything else. You need a clean working tree. (script auto-fails if not satisfied)
3. Run `./scripts/release.sh 1.2.1` – version bump guidelines:
   - Patch (1.2.0 → 1.2.1): bug fixes, minor tweaks
   - Minor (1.2.1 → 1.3.0): new features
   - Major (1.3.0 → 2.0.0): major launches
   - (Script bumps version in `package.json`, `tauri.conf.json`, `Cargo.toml`)
   - (Script moves `[Unreleased]` → `[1.2.1]` in changelog, commits, and tags)
4. Push: `git push origin main --tags`
   - (CI builds universal macOS binary, creates GitHub release, updates `latest.json`)
   - (Website auto-deploys, users get update notification on next check)

## How updates work

- App checks `https://getcmdr.com/latest.json` on start and every 60 min
- If newer version found → downloads silently → shows "Restart to update" toast
- Signatures verified with public key embedded in app

## Troubleshooting

### Release build failed, need to retry same version

Delete tag, fix the issue, commit, recreate tag, push:

```bash
git tag -d v0.3.0                      # delete local tag
git push origin :refs/tags/v0.3.0      # delete remote tag
# ... fix and commit ...
git tag v0.3.0                         # recreate tag
git push origin main --tags            # push again
```

### Draft release left on GitHub after failed build

Go to GitHub → Releases → delete the draft manually before retrying.
