# Releasing

How to release a new version of Cmdr. Use the `/release` command to start.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub secrets
- Self-hosted runner: `create-dmg` must be installed (`brew install create-dmg`)

## Pre-release smoke test on old macOS

Cmdr targets macOS 12 Monterey and up. The bundled Safari there can be 15.x, which doesn't support `color-mix()` (16.2+)
or `oklch` color (16.4+). We carry static sRGB fallbacks for both in `app.css` (`@supports not (color: color-mix(...))`
blocks) and via JS in `accent-color.ts` / `volume-tint.svelte.ts`. The fallbacks have to stay in sync as new tokens
land.

Before tagging a release:

1. On any Mac, run `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` from the repo root. This forces the fallback path on modern
   WebKit by faking `hasColorMix = false` (routes the JS branches through sRGB mix) and setting `data-force-old-webkit`
   on `<html>` (activates the mirror of the `@supports not (...)` blocks in `app.css`). It doesn't perfectly replicate
   Safari 15.x's renderer, but it does prove the fallback values look reasonable.
2. Optionally, boot a Monterey 12.7+ VM or a real old Mac and open the dev build. Note that ARM Monterey VMs ship with
   current Safari (17.x), so the bug isn't reproducible there without an early-12.x IPSW.
3. Either way, confirm the four user-visible spots aren't broken:
   - The "Open System Settings" button hovers to a lighter gold (not black).
   - The per-pane disk usage bar fills with green/orange/red instead of just the gray track.
   - The file-list cursor row has a visible gold-tinted background.
   - In dark mode, file-list size column shows the rainbow tier colors (not uniform gray).
4. Grep the app log for `Old WebKit detected:` — `logWebkitCompat()` emits this on startup when `color-mix()` isn't
   supported (or when the dev override is on). If you see it on Monterey 12.7+, the fallback path is doing its job.

If a new `color-mix()` token lands without a matching entry in the `@supports not` blocks, those four spots silently
break on old WebKit. Keep the lists in `app.css` in sync, and prefer the JS-derivation pattern (`accent-color.ts`,
`volume-tint.svelte.ts`) for any token that depends on the live macOS accent color.

## Keep the Mac awake during the build

The self-hosted runner lives on this Mac. If the machine sleeps (even briefly, or just the display), GitHub Actions
drops the runner connection and every in-flight matrix job fails with
`The self-hosted runner lost communication with the server.` This bit us on the 0.13.0 release: all three jobs failed at
exactly 11m1s each.

Before pushing the tag, start `caffeinate` in the background. The release script does NOT do this automatically; the
agent (or the human running the release) is responsible for arming and disarming it.

```bash
caffeinate -dimsu &              # -d display, -i idle, -m disk, -s on AC, -u user active
CAFFEINATE_PID=$!
# ... push the tag, monitor the build ...
kill $CAFFEINATE_PID             # once all matrix jobs are done (success or fail)
```

Agents: do this as a Bash `run_in_background` call right after the push, and `kill` it once the release monitor reports
the run has finished (wait for the overall run to be `completed`, not just the build matrix). If the release fails and
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
the publish job won't run, with no broken state.

To check notarization status manually:

```bash
xcrun notarytool info <SUBMISSION_ID> \
  --key ./_ignored/AuthKey_Apple_Cmdr.p8 \
  --key-id C9VUN857DD \
  --issuer 2c362f71-0680-4ec7-a74f-c62be656eeb7
```

The submission ID is logged in the build output before the timeout. Once the status shows `Accepted`, re-run the failed
job(s) in GitHub Actions; tauri-action will re-submit, Apple will return `Accepted` immediately (same binary hash), and
the build will complete in minutes.

Use "Re-run failed jobs" (not "Re-run all jobs") to avoid rebuilding architectures that already succeeded.

### Publish job failed but builds succeeded

The publish job downloads signatures from the release, generates `latest.json`, updates the release body, commits to
main, and triggers a website deploy. If it fails:

- **Missing signatures**: check that all 3 build jobs uploaded their `.sig` files. The publish job validates this
  upfront and fails fast with a clear message.
- **Git push failed**: another commit was pushed to main between checkout and push. Re-run the publish job; it does
  `git pull --rebase` to handle this, but if the rebase itself conflicts (someone else edited `latest.json`), it needs
  manual resolution.
- **Website deploy webhook failed**: re-trigger manually by pushing any commit to main, or SSH into the server and run
  the deploy script.

### `bundle_dmg.sh` hangs ~2 minutes then fails on every matrix job

The `actions-runner` auto-updated to a new version and its bundled `node` at
`~/actions-runner/externals.<version>/node20/bin/node` is a TCC client macOS has never seen. The first `osascript` call
in `bundle_dmg.sh` pops a "control Finder" prompt; if no one's at the keyboard, the prompt times out after ~2 minutes
and TCC records `auth_value=0` (denied) for that node path in `~/Library/Application Support/com.apple.TCC/TCC.db`.
Every subsequent DMG build hangs the same way until you flip the bit.

Recovery: trigger the prompt while you're at the keyboard and click Allow. Run this once:

```bash
NODE=~/actions-runner/externals/node20/bin/node
"$NODE" -e "require('child_process').execFileSync('/usr/bin/osascript', ['-e', 'tell application \"Finder\" to return name of startup disk'], { stdio: 'inherit' })"
```

A macOS dialog should appear within a second or two. Click Allow. From then on, every `osascript` call from this
runner-node path is authorized and `bundle_dmg.sh` runs cleanly until the runner auto-updates again.

`auth_value` codes in `TCC.db`: 0=denied, 1=ask, 2=allowed. Don't try to fix a stuck `auth_value=0` by `UPDATE`-ing the
row to 2 directly: tccd re-validates each row's `csreq` against the live binary's code signature on use, plus there's an
integrity layer on Sonoma+. A hand-edited row reads back fine via `SELECT` but tccd treats it as untrusted and
re-prompts. The only reliable path is to make the prompt fire.

Prevention: the `/release` agent prompt fires an `osascript`-via-runner-node canary right after the CHANGELOG draft so
the prompt lands while the user is at the keyboard. See step 3 of `.claude/commands/release.md`.

### `bundle_dmg.sh` fails fast (~3 s) on the universal/aarch64/x86_64 build

A leftover `/Volumes/Cmdr` mount (typically from a Finder double-click on an old DMG) makes the new bundle fail because
the volume name is already taken. Both `scripts/release.sh` and the release workflow detach `/Volumes/Cmdr*` mounts
before building, so this should be self-healing. If you hit it anyway (for example, you mounted a DMG between the
workflow's detach step and the actual build), detach manually and re-run failed jobs:

```bash
hdiutil detach /Volumes/Cmdr -force      # or "Cmdr 1", etc.
gh run rerun <release-run-id> --failed
```

### Tauri bundles unexpected binaries

Tauri's bundler includes all `[[bin]]` targets from the cmdr package, not just the main `Cmdr` binary. Dev-only tools
must live in separate workspace crates (like `crates/index-query/`) to stay out of the bundle. Non-`.rs` files in
`src/bin/` (like `CLAUDE.md`) also confuse the bundler; it strips the extension and tries to bundle the result as a
binary.
