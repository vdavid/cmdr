# Releasing

How to release a new version of Cmdr. An agent runs the whole flow via the `/release` command: it arms `caffeinate`,
monitors the build, verifies the public surface afterwards, and handles failure recovery. The human's role is to review
the CHANGELOG draft, confirm the version, and click any macOS permission prompts.

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

Before pushing the tag, make sure `caffeinate` is holding the Mac awake. The release script does NOT do this
automatically; the agent running the release is responsible for it.

**Check first, then arm only if needed.** A `caffeinate -dimsu` may already be running (a previous release, or the user
started one). Don't stack a second one, and don't kill one you didn't start.

```bash
if pgrep -lf 'caffeinate -dimsu' >/dev/null; then
    echo "caffeinate already running, leaving it"
else
    caffeinate -dimsu &          # -d display, -i idle, -m disk, -s on AC, -u user active
    CAFFEINATE_PID=$!
    # ... push the tag, monitor the build ...
    kill $CAFFEINATE_PID         # once all matrix jobs are done (success or fail)
fi
```

Agents: check with `pgrep -lf 'caffeinate -dimsu'` right after the push. If one's already running, skip arming and skip
the disarm at the end. Otherwise arm it as a Bash `run_in_background` call and `kill` it once the release monitor
reports the run has finished (wait for the overall run to be `completed`, not just the build matrix) - but only if you
armed it yourself. If the release fails and the user wants to re-run failed jobs with no caffeinate running, re-arm it
first.

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

### `codesign` fails with `errSecInternalComponent` (and `gh` stops working after a release)

`errSecInternalComponent` from `codesign` means the signing key can't be resolved or accessed cleanly. Three ways this
happened on the self-hosted runner:

- **The llama-server dylib signing in `beforeBuildCommand` leaned on the login keychain.** tauri-action's bundler sets
  up its own signing keychain from `APPLE_CERTIFICATE`, but only at bundling time; `download-llama-server.go` signs the
  bundled dylibs before that. The runner's launchd service runs with `SessionCreate=true` (GitHub's `svc.sh` plist), so
  its jobs live in their own security session where the login keychain's private key isn't usable (the exact same
  `codesign` command works from a GUI shell), and every matrix job failed ~30 s in. A runner-service restart doesn't
  help. The fix in `release.yml` ("Set up llama-server signing keychain") imports the cert into a dedicated keychain
  that the Go script targets explicitly via `codesign --keychain` (`LLAMA_SIGN_KEYCHAIN`). The keychain must ALSO be in
  the user keychain search list: `--keychain` alone fails with the same `errSecInternalComponent` for a keychain outside
  the search list (verified empirically on this runner). The explicit `--keychain` is what keeps the login keychain's
  copy of the identity from making resolution ambiguous; the "Restore keychain search list" cleanup step resets the list
  afterwards.

The other two are about the **same Developer ID identity being reachable from more than one keychain in the search
list** (ambiguous resolution):

- **A duplicate cert across keychains.** The Developer ID Application cert existed in both the login keychain (with its
  private key) and the System keychain (a stray keyless copy). Check with `security find-identity -v -p codesigning`: if
  the same identity (same SHA-1) appears twice, that's the cause. Remove the stray copy from the offending keychain, for
  example `sudo security delete-certificate -Z <SHA1> /Library/Keychains/System.keychain`. The login keychain copy (the
  one with the private key) is the one to keep. Verify local signing still works: `codesign -s <SHA1> --force /tmp/x`.
- **Double import in the workflow.** An earlier version of `release.yml` imported the cert manually _and_ let
  tauri-action's bundler import it too, putting the cert in two keychains in the search list. The bundler now owns
  signing on its own (no manual `security import` step) so only one keychain holds the cert. Don't reintroduce a manual
  cert-import step.

The companion symptom is **`gh` reporting an invalid token after a release**. `gh` stores its OAuth token in the login
keychain (secure storage, no `oauth_token` in `~/.config/gh/hosts.yml`). The old manual signing step ran
`security list-keychain -d user -s <temp>`, which _replaced_ the user search list and dropped the login keychain, so
`gh` (and any keychain-backed tool) couldn't find its token until the list was restored. The token is never actually
lost. Restore it with `security list-keychains -d user -s "$HOME/Library/Keychains/login.keychain-db"`. The workflow's
`Restore keychain search list` cleanup step now does this automatically on every release (`if: always()`).

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
