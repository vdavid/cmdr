# Releasing

How to release a new version of Cmdr. An agent runs the whole flow via the `/release` command: it arms `caffeinate`,
monitors the build, verifies the public surface afterwards, and handles failure recovery. The human's role is to review
the CHANGELOG draft, confirm the version, and click any macOS permission prompts.

Related guides for the signing and distribution steps: `apple-signing-and-notarization.md` and `homebrew-cask.md`.

## Prerequisites

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in GitHub secrets

## Which runner builds the release

Two runners can build Cmdr, and the choice is one line in `release.yml` (`build.runs-on`).

**GitHub-hosted (`macos-latest`) is what runs today.** The self-hosted Mac is registered and can be re-enabled, but its
service is stopped.

- **Why hosted won**: `bundle_dmg.sh` drives Finder over AppleScript, which on the self-hosted Mac needs a TCC
  Automation grant for the runner's bundled `node`. That path changes on every runner auto-update, so the grant lapses
  silently, and once a prompt times out unattended the entry sticks at denied with no supported way to clear it short of
  a machine-wide `tccutil reset AppleEvents`. That failure took all three matrix jobs on the 0.37.0 release; the
  troubleshooting section below has the full anatomy. Hosted images have no such gate.
- **What hosted costs**: every job is ephemeral, so each pays a cold ~1000-crate Tauri compile instead of reusing a warm
  cargo cache, and GitHub bills macOS minutes at a 10x multiplier. Partly offset by the three arch jobs running in
  parallel rather than queueing on one machine, so wall-clock is roughly one cold build.
- **What hosted needs that self-hosted didn't**: `brew install create-dmg` as a step (the Mac had it installed by hand),
  and mise's action cache left on (there's no local tool dir to persist).

**To switch back**: set `runs-on: [self-hosted, macOS, ARM64]`, restart the service with
`cd ~/actions-runner && ./svc.sh start`, and fix the Finder Automation grant first, or the DMG step hangs ~2 minutes and
fails every job. Check the runner version too: `tauri-action` v1 declares `runs.using: node24`, which needs
`actions-runner` 2.327.0 or newer (added in that release, 2025-07-22); an older runner rejects the step outright. Worth
doing if hosted minutes get expensive or cold builds get painful; the whole workflow still carries the
self-hosted-specific guards (the stale-`/Volumes/Cmdr` detach, the keychain search-list restore), so nothing else has to
change.

Re-enabling the persistent cargo target dir belongs with that switch, not before it: the old `CARGO_TARGET_DIR`
(`~/.cache/cmdr-release-target`, outside the workspace `actions/checkout` wipes) was safe ONLY because the jobs ran
sequentially on one machine. Under hosted runners they run concurrently, where a shared target dir would have cargo
locking and corrupting it. The directory is gone (reclaimed 21 GB); switching back recreates it, at the cost of one cold
compile.

## Release gates that abort before tagging

`scripts/release.sh` runs a few hard gates locally before it commits and tags, so a release that can't ship is never
tagged. Beyond the version/CHANGELOG checks and `oxfmt --ci`, two are worth knowing:

- **Visual baselines are auto-refreshed (Docker required).** After the CHANGELOG/roadmap/version are finalized, the
  script runs `apps/website/scripts/update-visual-baselines.sh`, which re-shoots any website visual baseline that
  release-prep copy (mainly `feature-status.json` → `/features`) made stale, on both macOS and (in a pinned Playwright
  container) Linux, and folds them into the release commit. Docker must be running; a stopped Docker aborts the release
  before tagging. Without this, the stale `-linux` baseline turns CI red right after the tag. Mechanism:
  `apps/website/DETAILS.md` § Visual baselines.
- **No stale translations may ship.** A non-`en` translation whose stored `@key.sourceHash` no longer matches the
  current English value is STALE (it renders text translated from a sentence that no longer exists). The
  `desktop-i18n-stale` check is warn-only in normal `pnpm check` (a maintenance signal, not a daily-dev build breaker),
  but the release script escalates it to a build-failing ERROR by running
  `CMDR_I18N_STALE_STRICT=1 pnpm check i18n-stale`. With `set -e`, a stale finding aborts the release before tagging, so
  the re-translation lands first. Fix: re-translate the changed keys and refresh `@key.sourceHash` (and re-review), then
  re-run the release. English-only today, so this is a clean no-op until a real locale exists. Mechanism and schema:
  `apps/desktop/src/lib/intl/messages/DETAILS.md` § `@key` metadata schema.

## Pre-release smoke test on old macOS

Cmdr opens on macOS 10.15 Catalina and up, which means two different old-WebKit paths, and both want a look before a
tag. The floor rationale and version evidence: `docs/notes/system-requirements-and-es2025.md`.

### Degraded but working: the `color-mix()` fallbacks

Safari below 16.2 doesn't support `color-mix()`, and below 16.4 not `color-mix(in oklch, …)`. We carry static sRGB
fallbacks in `app.css` (`@supports not (color: color-mix(...))` blocks) and via JS in `accent-color.ts` /
`volume-tint.svelte.ts`. They have to stay in sync as new tokens land.

1. On any Mac, run `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` from the repo root. This fakes `hasColorMix = false` (routing
   the JS branches through sRGB mix) and sets `data-force-old-webkit` on `<html>` (activating the mirror of the
   `@supports not (...)` blocks). It doesn't replicate Safari 15.x's renderer, but it proves the fallback values look
   reasonable.
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

### Below the floor: the boot guard

Under Safari 15.4 the app can't run at all, and the inline guard in `apps/desktop/src/app.html` replaces the window with
a translated "Cmdr needs a newer Safari" screen instead of leaving a white one. Nobody on the team has that Safari, so
the dev override is the only routine way to see it.

1. Run `VITE_CMDR_FORCE_OLD_WEBKIT=unsupported pnpm dev` from the repo root. The flag is read at BUILD time by
   `svelte.config.js`, so it has to be set before the dev server starts; restarting an already-running one won't pick it
   up.
2. Confirm the screen paints: title, one paragraph, one Quit button, centered, correct in both light and dark mode.
3. Click Quit. The app should exit. (It goes through `plugin:process|exit`, so the quit gate sees it like ⌘Q.)
4. Check a second language: set macOS to one of the shipped locales, or run the same command after temporarily pointing
   `navigator.language`'s answer at another tag in the dev tools. Copy comes from `main.oldWebkit.*` in the catalog, so
   any locale you can pick in Settings is one the guard can show.

Nothing here needs a real old Mac: `apps/desktop/scripts/app-boot-guard.test.ts` runs the actual guard with each Safari
15.4 capability removed in turn, and fails on ES6 syntax that old WebKit couldn't parse. The smoke test is about how the
screen LOOKS, which no test can judge.

## Keep the Mac awake during the build (self-hosted only)

**Not needed while `runs-on` is `macos-latest`**: the build happens on GitHub's hardware, so this Mac can sleep, and the
laptop can close, with no effect on the release. Everything below applies the moment the runner switches back to
self-hosted.

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

## Refreshing the app-directory listings (optional, minor and major releases)

Cmdr is listed on app directories (MacUpdate, AlternativeTo), each with a file in `brand/listings` holding every field
of that site's form, filled and ready to paste. Those files are the source of truth: edit them first, then paste from
them. Never retype a listing from the CHANGELOG at the form.

This is optional and deliberately not part of the release script:

- **Skip it for patch releases.** A patch's changelog isn't interesting enough to spend a review cycle on, and the
  listing pages age gracefully.
- **The download URL never goes stale**, so an outdated listing still hands visitors the current DMG. Only the version
  string and the changelog text on the page age. (`getcmdr.com/download/latest/<arch>` resolves at request time; see
  `apps/api-server/src/telemetry/DETAILS.md` § Download tracking.)
- **No directory offers an API**, so submitting is a human pasting into a web form. An agent prepares the text and stops
  there: submitting is an external action.

For a minor or major release, the agent updates `brand/listings/macupdate.md` in place:

- The version number.
- The "Version changes" HTML, rewritten from the new CHANGELOG section into their `<h5>` + `<ul>` format (New / Improved
  / Fixed). Cover the whole minor line, patches included, since the listing skipped those.
- The description, but only where the release actually changed it: a feature that graduated out of alpha, a claim that
  no longer holds, a "coming soon" that shipped. Leave the wording alone otherwise; David reviews every human-facing
  string, and needless churn costs him a review.

Then hand David the submission link: https://member.macupdate.com/content/submit, where "Modify an existing listing?" at
the top takes the app name and loads the current listing. MacUpdate prefers updating an existing listing over a new one
(downloads keep accumulating, version history stays catalogued, and Watch List users get notified).

## What a release publishes

One GitHub release per tag, carrying these assets for each of the three arches (`aarch64`, `x64`, `universal`):

- `Cmdr_<version>_<arch>.dmg`: what people download. `getcmdr.com/download/latest/<arch>`, the website's download
  buttons, and the Homebrew cask each rebuild this name from the version, so it's the one asset name the rest of the
  repo hard-codes.
- `Cmdr_<version>_<arch>.app.tar.gz` plus its `.sig`: the updater payload and its minisign signature.
- `latest.json`, whose copy in `apps/website/public/latest.json` (committed by the publish job) is what
  `getcmdr.com/latest.json` serves.

Two naming details are load-bearing:

- **File names say `x64`, never `x86_64`.** Tauri's CLI names the Intel bundles that way, while the rest of the repo
  (URL paths, D1 columns, Rust target triples) says `x86_64`. The mapping happens at the file-name boundary only; see
  `apps/api-server/src/telemetry/DETAILS.md` § Gotchas.
- **Every bundle name carries the app version, `.app.tar.gz[.sig]` included.** That last part arrived with
  `tauri-action` v1 (verified on tauri-action v1.0.0, reading its `getAssetName` name builder, 2026-09-02); v0 left the
  tarballs unversioned, so releases up to 0.41.0 carry `Cmdr_<arch>.app.tar.gz` instead.

**The publish job owns `latest.json`, not `tauri-action`.** From v1 the action writes download URLs in GitHub's API form
(`api.github.com/repos/.../releases/assets/<id>`), which hands back the binary only to a caller sending
`Accept: application/octet-stream`. Cmdr's updater does a plain `reqwest` GET (`download_update` in
`apps/desktop/src-tauri/src/updater/mod.rs`), so it would read JSON metadata and fail signature verification. The
workflow passes `uploadUpdaterJson: false` and builds the manifest itself with
`https://github.com/<repo>/releases/download/<tag>/<file>` URLs. Before uploading it, the job asserts that every URL in
it names an asset actually on the release: a name that drifts from what the action uploaded would strand every install
in the field, with no fallback in the app to recover.

## How updates work

- App checks `https://getcmdr.com/latest.json` on start and every 60 min
- If newer version found → downloads silently → shows "Restart to update" toast
- Signatures verified with public key embedded in app

## The tag guard, and rolling back

Pushing a `v*` tag runs the whole Release workflow, and `publish` rewrites `latest.json` for whatever tag fired it. It
has no idea which version that is, so re-pushing an old tag points the entire install base at an old build. The `guard`
job gates the workflow on one rule: the tag's version must be strictly greater than the version
`apps/website/public/latest.json` currently advertises. It runs in seconds and blocks `build`, so a mistaken push never
reaches the three 90-minute macOS jobs and never overwrites assets on an old release.

Two consequences worth knowing before they surprise you:

- **Re-pushing the current version is refused.** That's a rebuild of something users already have. Retrying a _failed_
  build is unaffected: publish never ran, so the manifest still names the previous version and the tag is still ahead of
  it.
- **A full re-run after publish already committed the manifest is refused too**, because by then the manifest names this
  very version. Use the override below, or re-run only the failed jobs rather than all of them.

**To roll back deliberately** (a shipped release turns out to be bad and you want everyone back on the previous one):

1. Set the repository variable `RELEASE_REPUBLISH_TAG` to the exact tag you're republishing, like `v0.41.0`. It's under
   Settings → Secrets and variables → Actions → Variables. It has to match the tag exactly, so a leftover value can
   never act as a blanket "always allow".
2. Push or re-run that tag. The guard logs a warning saying the downgrade is deliberate, and the release proceeds.
3. **Clear the variable.** Nothing expires it for you.

The variable lives outside git on purpose. A tag push is the mistake this guards against, so no combination of tag
pushes should be able to unlock it.

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

The `guard` job doesn't get in the way here: the failed run never published, so `latest.json` still names the previous
version and this tag is still ahead of it. It only refuses once the manifest already advertises this same version, at
which point the retry would be a rebuild of something users have.

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

### The release notes reverted to "See CHANGELOG.md for details."

`tauri-action` rewrites an existing release's name and body on every run (since v1). A build job re-run after the
publish job already wrote the changelog therefore puts the placeholder body back. Re-run the publish job afterwards: it
re-extracts the CHANGELOG section, regenerates `latest.json`, and re-commits the website copy.

### Publish job failed but builds succeeded

The publish job downloads signatures from the release, generates `latest.json`, updates the release body, commits to
main, and triggers a website deploy. If it fails:

- **Missing signatures**: check that all 3 build jobs uploaded their `.sig` files. The publish job validates this
  upfront and fails fast with a clear message. It looks for `Cmdr_<version>_<arch>.app.tar.gz.sig`, so a `tauri-action`
  bump that changes bundle naming lands here first.
- **`latest.json` points at an asset that isn't on the release**: same cause, caught by the assertion the job runs
  before uploading the manifest. Compare `gh release view <tag> --json assets` against the names the workflow builds,
  and fix the workflow rather than the release.
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

### `bundle_dmg.sh` hangs ~2 minutes then fails on every matrix job (self-hosted only)

This is the failure that moved releases to GitHub-hosted runners (see § "Which runner builds the release"). It can't
happen on a hosted image; read on only when running self-hosted.

The `actions-runner` auto-updated to a new version and its bundled `node` at
`~/actions-runner/externals.<version>/node20/bin/node` is a TCC client macOS has never seen. The first `osascript` call
in `bundle_dmg.sh` pops a "control Finder" prompt; if no one's at the keyboard, the prompt times out after ~2 minutes
and TCC records `auth_value=0` (denied) for that node path in `~/Library/Application Support/com.apple.TCC/TCC.db`.
Every subsequent DMG build hangs the same way until you flip the bit.

Read the state first, resolving the REAL path (`externals/` is a symlink into `externals.<version>/`, and tccd keys its
rows on the resolved path):

```bash
NODE=$(readlink -f ~/actions-runner/externals/node20/bin/node)
sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
  "SELECT auth_value FROM access WHERE client='$NODE' AND service='kTCCServiceAppleEvents' AND indirect_object_identifier='com.apple.finder';"
```

`auth_value` codes: 0=denied, 1=ask, 2=allowed. Empty or `1` just needs someone at the keyboard when the next build
runs. `2` is fine. `0` blocks every DMG build until it's cleared, and clearing it is the hard case:

- **The `externals/` symlink carries its OWN row**, left at `2` by earlier grants, so a check against the symlink path
  reports "allowed" while the resolved path sits at `0`. ❌ Never read the state through the symlink. This masked a real
  denial on the 0.37.0 release and cost all three matrix jobs.
- **You cannot fire the prompt from a Terminal or agent shell.** TCC attributes the request to the responsible process,
  which there is the already-granted shell, so `osascript` succeeds without ever asking about node and no row changes.
  Only the runner's launchd service (`SessionCreate=true`) puts node in that role, which is why the prompt appears
  during a real build and nowhere else.
- **System Settings → Privacy & Security → Automation may refuse to flip it.** A stuck `0` row's Finder checkbox can
  bounce straight back off (observed on the 0.37.0 release, macOS 26.5.2). Several older runner-node entries sit there
  allowed, which makes the broken one easy to miss.
- **`tccutil` can't target it**: it takes a bundle identifier, and this client is a bare binary path
  (`tccutil: No such bundle identifier`). The only supported clear is `tccutil reset AppleEvents` with no argument,
  which drops EVERY app's Automation grant on the machine, so every one of them re-prompts on next use. Get the user's
  explicit consent before running it; it's their whole system, not just the runner.
- ❌ Don't `UPDATE` the row to 2 by hand: tccd re-validates each row's `csreq` against the live binary's signature, plus
  there's an integrity layer on Sonoma+. It reads back fine via `SELECT` and still behaves as untrusted.

After a reset, start a build with the user at the keyboard: the prompt lands within a second or two of
`Running bundle_dmg.sh`, and one Allow authorizes that runner-node path until the runner auto-updates again.

Prevention: step 3 of `.claude/commands/release.md` reads the resolved path's `auth_value` right after the CHANGELOG
draft, so a denial is found before anything is tagged rather than after three jobs burn.

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
