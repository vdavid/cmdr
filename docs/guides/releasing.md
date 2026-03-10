# Releasing

How to release a new version of Cmdr.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub secrets

## Release steps

1. Ask an agent to update the changelog with this prompt:
   ```
   I'm doing a release based on @docs/guides/releasing.md. Update CHANGELOG.md based on git commits since last release.
   Read the existing changelog to match its style. Note: commits have title + body – read all!
   You can link multiple commits for changelog items if needed. List major but non-app changes in a "Non-app" section.
   ```
2. Run `./scripts/release.sh 0.x.x` (uncommitted changelog changes are fine — they get included in the release commit). Version bump guidelines:
   - Patch (1.2.0 → 1.2.1): bug fixes, minor tweaks
   - Minor (1.2.1 → 1.3.0): new features
   - Major (1.3.0 → 2.0.0): major launches
   - (Script bumps version in `package.json`, `tauri.conf.json`, `Cargo.toml`)
   - (Script moves `[Unreleased]` → `[0.x.x]` in changelog, commits, and tags)
3. Push: `git push origin main --tags`
   - (CI builds universal macOS binary, creates GitHub release, updates `latest.json`)
   - (Website auto-deploys, users get update notification on next check)
4. Track the build at https://github.com/vdavid/cmdr/actions

## How updates work

- App checks `https://getcmdr.com/latest.json` on start and every 60 min
- If newer version found → downloads silently → shows "Restart to update" toast
- Signatures verified with public key embedded in app

## Troubleshooting

### Release build failed, need to retry same version

Delete tag, fix the issue, commit, recreate tag, push:

```bash
git tag -d v0.x.x                      # delete local tag
git push origin :refs/tags/v0.x.x      # delete remote tag
# ... fix and commit ...
git tag v0.x.x                         # recreate tag
git push origin main --tags            # push again
```

### Draft release left on GitHub after failed build

Go to GitHub → Releases → delete the draft manually before retrying.

### Apple notarization is slow (builds time out at 30 min)

Apple's notarization can take anywhere from minutes to 20+ hours. If the build job times out waiting
for notarization, the publish job won't run — no broken state.

To check notarization status manually:

```bash
xcrun notarytool info <SUBMISSION_ID> \
  --key ./_ignored/AuthKey_Apple_Cmdr.p8 \
  --key-id C9VUN857DD \
  --issuer 2c362f71-0680-4ec7-a74f-c62be656eeb7
```

The submission ID is logged in the build output before the timeout. Once the status shows `Accepted`,
re-run the failed job(s) in GitHub Actions — tauri-action will re-submit, Apple will return `Accepted`
immediately (same binary hash), and the build will complete in minutes.

Use "Re-run failed jobs" (not "Re-run all jobs") to avoid rebuilding architectures that already
succeeded.

### Publish job failed but builds succeeded

The publish job downloads signatures from the release, generates `latest.json`, updates the release
body, commits to main, and triggers a website deploy. If it fails:

- **Missing signatures**: check that all 3 build jobs uploaded their `.sig` files. The publish job
  validates this upfront and fails fast with a clear message.
- **Git push failed**: another commit was pushed to main between checkout and push. Re-run the
  publish job — it does `git pull --rebase` to handle this, but if the rebase itself conflicts
  (someone else edited `latest.json`), it needs manual resolution.
- **Website deploy webhook failed**: re-trigger manually by pushing any commit to main, or SSH into
  the server and run the deploy script.
