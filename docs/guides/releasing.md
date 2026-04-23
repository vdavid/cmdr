# Releasing

How to release a new version of Cmdr. Use the `/release` command to start.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub secrets
- Self-hosted runner: `create-dmg` must be installed (`brew install create-dmg`)

## Keep the Mac awake during the build

The self-hosted runner lives on this Mac. If the machine sleeps — even briefly, or just the display — GitHub Actions
drops the runner connection and every in-flight matrix job fails with
`The self-hosted runner lost communication with the server.` This bit us on the 0.13.0 release: all three jobs failed at
exactly 11m1s each.

Before pushing the tag, start `caffeinate` in the background. The release script does NOT do this automatically — the
agent (or the human running the release) is responsible for arming and disarming it.

```bash
caffeinate -dimsu &              # -d display, -i idle, -m disk, -s on AC, -u user active
CAFFEINATE_PID=$!
# ... push the tag, monitor the build ...
kill $CAFFEINATE_PID             # once all matrix jobs are done (success or fail)
```

Agents: do this as a Bash `run_in_background` call right after the push, and `kill` it once the release monitor reports
the run has finished (not just the build matrix — wait for the overall run to be `completed`). If the release fails and
the user wants to re-run failed jobs, re-arm caffeinate first.

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

Apple's notarization can take anywhere from minutes to 20+ hours. If the build job times out waiting for notarization,
the publish job won't run — no broken state.

To check notarization status manually:

```bash
xcrun notarytool info <SUBMISSION_ID> \
  --key ./_ignored/AuthKey_Apple_Cmdr.p8 \
  --key-id C9VUN857DD \
  --issuer 2c362f71-0680-4ec7-a74f-c62be656eeb7
```

The submission ID is logged in the build output before the timeout. Once the status shows `Accepted`, re-run the failed
job(s) in GitHub Actions — tauri-action will re-submit, Apple will return `Accepted` immediately (same binary hash), and
the build will complete in minutes.

Use "Re-run failed jobs" (not "Re-run all jobs") to avoid rebuilding architectures that already succeeded.

### Publish job failed but builds succeeded

The publish job downloads signatures from the release, generates `latest.json`, updates the release body, commits to
main, and triggers a website deploy. If it fails:

- **Missing signatures**: check that all 3 build jobs uploaded their `.sig` files. The publish job validates this
  upfront and fails fast with a clear message.
- **Git push failed**: another commit was pushed to main between checkout and push. Re-run the publish job — it does
  `git pull --rebase` to handle this, but if the rebase itself conflicts (someone else edited `latest.json`), it needs
  manual resolution.
- **Website deploy webhook failed**: re-trigger manually by pushing any commit to main, or SSH into the server and run
  the deploy script.

### Tauri bundles unexpected binaries

Tauri's bundler includes all `[[bin]]` targets from the cmdr package, not just the main `Cmdr` binary. Dev-only tools
must live in separate workspace crates (like `crates/index-query/`) to stay out of the bundle. Non-`.rs` files in
`src/bin/` (like `CLAUDE.md`) also confuse the bundler — it strips the extension and tries to bundle the result as a
binary.
