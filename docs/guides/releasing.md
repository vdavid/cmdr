# Releasing

How to release a new version of Cmdr. An agent runs the whole flow via the `/release` command: it arms `caffeinate`,
monitors the build, verifies the public surface afterwards, and handles failure recovery. The human's role is to review
the CHANGELOG draft, confirm the version, and click any macOS permission prompts.

Related guides for the signing and distribution steps: `apple-signing-and-notarization.md` and `homebrew-cask.md`.

## Prerequisites

- The signing secrets in the `release` GitHub environment (next section).
- Where the update archives get signed, set by the `RELEASE_UPDATE_SIGNING` variable: in `ci` mode the updater key is
  among those secrets, in `local` mode it's in this laptop's sops store (§ Who signs the update archives).

## Signing secrets

The secrets that sign, notarize, and updater-sign a build live in the `release` environment on `vdavid/cmdr`, and only
the `build` job declares `environment: release`. The environment's deployment policy admits `v*` tags only, with no
required reviewers (releases run unattended), so once the repo-level copies are gone (below), a workflow on a branch
can't reach them. Where each value comes from: the vault note `projects/Cmdr/workflow/Cmdr signing keys.md`.

- **In the environment** (set 2026-10-05): `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`,
  `APPLE_API_KEY`, `APPLE_API_KEY_BASE64`, `APPLE_API_ISSUER`, `APPLE_SIGNING_IDENTITY`. The updater key was checked
  against the app's key ID `A2601F36BB168C0A` and the notarization trio against Apple's notary API before setting.
- **Still repo-level only**: `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`. Their only copy outside GitHub is in
  Bitwarden, which agents can't reach. Set them from there:
  `gh secret set APPLE_CERTIFICATE --env release -R vdavid/cmdr` (it prompts for the value), same for the password.
- **Not set yet**: `APPLE_INSTALLER_CERTIFICATE` and `APPLE_INSTALLER_CERTIFICATE_PASSWORD`, the Developer ID Installer
  certificate that signs the `.pkg`. Environment-only from the start (no repo-level copy). Until they exist the `pkg`
  job skips (§ The installer package).
- **Repo-level copies still exist** for all eight. A job in an environment sees both, and the environment value wins on
  a name clash, so nothing breaks while the move is partial.

**Follow-up once the first release through the environment succeeds** (with all eight in the environment): delete the
repo-level copies, `gh secret delete <NAME> -R vdavid/cmdr` for each of the eight, then confirm the next release still
signs and notarizes. The updater pair (`TAURI_SIGNING_*`) leaves GitHub entirely once a `local` release has shipped: §
Who signs the update archives.

## Who signs the update archives

The updater key (minisign, key ID `A2601F36BB168C0A`) is the one release secret that can't be rotated: its public half
is compiled into every installed copy, so whoever holds the private half can ship an update to every Mac, for good. The
repository variable `RELEASE_UPDATE_SIGNING` picks where it signs, read by the `guard` job of each tag push:

- **`ci`, or unset**: the tag push does everything, and the `build` job signs each archive with `TAURI_SIGNING_*` from
  the `release` environment.
- **`local`**: the key never leaves this laptop. Decided 2026-10-06; the threat-model discussion is private issue
  `vdavid/cmdr-reports#39`.

Read it with `gh variable get RELEASE_UPDATE_SIGNING -R vdavid/cmdr` (an error means unset, so `ci`). Any other value
fails the guard. Both modes stay implemented in the same files (`release-pipeline.yml`, `release.yml`), so switching is
the variable alone.

### How a `local` release runs

1. `release.sh` and the tag push are unchanged.
2. **The tag push's run** builds without publishing: `guard`, `ci-gate`, then `draft`, which creates the draft release
   up front (three parallel builds each looking for a draft by tag could each create one; drafts can't be fetched by
   tag). The three `build` jobs upload the DMGs and `.app.tar.gz` archives into it with no `.sig` files, `sbom` runs
   beside them, and `attest` attests the draft's assets. `publish` and `bump-tap` skip, so `latest.json` doesn't move
   and nothing is public.
   - The Tauri CLI won't bundle without an updater key once the config has a pubkey, and the archive must still come out
     of the bundler in the shape `ci` mode ships. So each build job makes a throwaway key (a mismatch the bundler only
     warns about) and keeps its signatures on the runner (`uploadUpdaterSignatures: false`).
3. **On the laptop, `./scripts/release-finish.sh X.Y.Z`** (the `/release` skill starts it right after the push). It
   waits for that run to go green, downloads the three archives, and runs `gh attestation verify` on each, pinned to
   `release-pipeline.yml` at the tag's commit (`--signer-digest`, `--source-digest`, `--source-ref`) on a GitHub-hosted
   runner. It refuses to sign anything that doesn't verify. Then it signs with `tauri signer sign`, the key and password
   read from sops (`CMDR_TAURI_SIGNING_PRIVATE_KEY`, `CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) into the signer's
   environment only, verifies each new signature against the pubkey in `tauri.conf.json` with its own minisign verifier
   (the check the app runs), uploads the `.sig` files, reads them back, and dispatches `release.yml` on the tag.
4. **The dispatched run** finishes it: `guard` (it also refuses a dispatch on a branch), then `publish`, which verifies
   every archive's `.sig` against the app's public key, publishes the draft, and then runs exactly the `ci` mode steps
   (`latest.json` from the `.sig` assets, checksums, release notes, the commit to `main`, the website deploy), then
   `attest` and `bump-tap`. `ci-gate`, `build`, and `sbom` skip.

`release-finish.sh` is resumable: re-run it after anything (a closed laptop, a red job). It holds no state of its own
and reads where the release stands on GitHub. A build still running gets waited for, a draft gets signed (re-signing a
draft is harmless, since nothing reads its signatures yet), a published release skips to the finishing run, a red
finishing run gets its failed jobs re-run once (a fresh dispatch would meet the guard, since `publish` may have moved
the manifest already), and a green one ends it. It doesn't read the variable: a release finishes in the mode its tag
push ran in.

**Dry run**: `./scripts/release-finish.sh -dry-run -out <dir> X.Y.Z` stops after verifying the new signatures and
touches nothing on GitHub; it works on a published release too. `-signer-workflow` (dry runs only) checks a release from
before the reusable workflow, which `release.yml` signed. (Verified on v0.50.0, 2026-10-06: provenance verified with
`-signer-workflow vdavid/cmdr/.github/workflows/release.yml`, the sops key signed all three archives and each signature
verified against the app's pubkey; the default pin refused the same archives; the verifier accepted v0.50.0's CI-made
signatures and rejected one swapped across arches.)

### What `local` mode protects, and what it doesn't

- **Protects against the key leaking from GitHub**: an exfiltrated secret, a workflow on a branch, or a compromised
  action step reading its environment. Once the GitHub copies are deleted, there's no key on GitHub to take.
- **Protects against an archive swapped on the release**: one that wasn't built by the pipeline at the signed tag has no
  matching provenance, so the laptop refuses it, and one swapped after signing fails the app's signature check.
- **Doesn't protect against a malicious build from inside the pipeline**: a compromised third-party action or dependency
  in the `build` job produces an archive with valid provenance, and the laptop signs it. Provenance proves where an
  archive was built, not what went into it.
- **Moves the key's risk to the laptop**, which holds it in sops (the age key in its Keychain) either way.

### Attestations in `local` mode

`attest` runs twice. The tag push attests the draft's assets (DMGs, archives, SBOMs) before anything is signed, which is
what the laptop verifies; the draft's bytes are final, so "attest as published" still holds. The dispatch attests the
published release again, which adds `latest.json` and `checksums.txt` (and repeats the rest, which is harmless). It
skips the `.sig` files: the laptop made them, so provenance naming the workflow would be false, and a minisign signature
is its own proof of origin.

### Switching modes

- **To `local`**: `gh variable set RELEASE_UPDATE_SIGNING -R vdavid/cmdr --body local`, before pushing the tag.
- **Back to `ci`** (the revert):
  1. `gh variable set RELEASE_UPDATE_SIGNING -R vdavid/cmdr --body ci` (or `gh variable delete`).
  2. If the GitHub copies of the key were already deleted, put them back in the `release` environment from sops, piped
     so the values never reach the shell history or the screen:
     `secret CMDR_TAURI_SIGNING_PRIVATE_KEY | gh secret set TAURI_SIGNING_PRIVATE_KEY --env release -R vdavid/cmdr`,
     then the same for `CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD` into `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
  3. Release as usual. The `/release` skill reads the variable and skips `release-finish.sh` in `ci` mode.

### After the first `local` release ships

The `TAURI_SIGNING_*` secrets stay in the `release` environment (and at repo level) until then, so `ci` mode keeps
working while the new flow proves itself. Until they're gone, `local` mode protects nothing against a leak from GitHub.
Once the first `local` release has passed the `/release` skill's checks (the manifest, the three archive URLs, the
attestations) and an installed copy has updated to it:

1. Delete the updater pair from GitHub: `gh secret delete TAURI_SIGNING_PRIVATE_KEY --env release -R vdavid/cmdr`, the
   same for `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and both repo-level copies (`gh secret delete <NAME> -R vdavid/cmdr`).
   From then on, going back to `ci` takes step 2 of the revert.
2. Update § Signing secrets and the release-chain bullets in `docs/threat-model.md`, which are phrased for this point.

## Which runner builds the release

**GitHub-hosted (`macos-26`) builds every release**, set in one line in `release-pipeline.yml` (`build.runs-on`). This
repo is public, so hosted macOS minutes are free. No self-hosted runner is registered, and nothing needs one.

- **Why the image and not `macos-latest`**: an image carries exactly one Xcode major, so that line picks the macOS SDK
  the bundle links against, and Apple gates behavior changes on it. A build step asserts the SDK major and fails before
  the compile, so a GitHub image bump can't quietly change the shipped app. What an SDK move would cost today, and what
  to check before making one: `apps/desktop/src-tauri/src/menu/DETAILS.md` § SF Symbol icons.

- **Why hosted, and why going back is unappealing**: `bundle_dmg.sh` drives Finder over AppleScript, which on a
  self-hosted Mac needs a TCC Automation grant for the runner's bundled `node`. That path changes on every runner
  auto-update, so the grant lapses silently, and once a prompt times out unattended the entry sticks at denied with no
  supported way to clear it short of a machine-wide `tccutil reset AppleEvents`. That failure took all three matrix jobs
  on the 0.37.0 release; the troubleshooting section below has the full anatomy. Hosted images have no such gate. A
  self-hosted Mac also has to stay awake for the whole build, or every in-flight job dies with
  `The self-hosted runner lost communication with the server`.
- **What hosted costs**: every job is ephemeral, so each pays a cold ~1000-crate Tauri compile instead of reusing a warm
  cargo cache. Partly offset by the three arch jobs running in parallel rather than queueing on one machine, so
  wall-clock is roughly one cold build.
- **What hosted needs that self-hosted didn't**: `brew install create-dmg` as a step, and mise's action cache left on
  (there's no local tool dir to persist).

**If a self-hosted runner ever earns its keep again**, it's a from-scratch setup: register one with a token from
`gh api -X POST repos/vdavid/cmdr/actions/runners/registration-token`, run `./config.sh` and `./svc.sh install`, then
set `runs-on: [self-hosted, macOS, ARM64]`. Fix the Finder Automation grant before the first run, or the DMG step hangs
~2 minutes and fails every job. Mind the runner version too: `tauri-action` v1 declares `runs.using: node24`, which
needs `actions-runner` 2.327.0 or newer (added in that release, 2025-07-22); an older runner rejects the step outright.
The workflow still carries its self-hosted-specific guards (the stale-`/Volumes/Cmdr` detach, the keychain search-list
restore), so nothing else has to change.

Re-enabling the persistent cargo target dir belongs with that switch, not before it: the old `CARGO_TARGET_DIR`
(`~/.cache/cmdr-release-target`, outside the workspace `actions/checkout` wipes) was safe ONLY because the jobs ran
sequentially on one machine. Under hosted runners they run concurrently, where a shared target dir would have cargo
locking and corrupting it. The directory is gone (reclaimed 21 GB); switching back recreates it, at the cost of one cold
compile.

## Release gates that abort before tagging

`scripts/release.sh` runs a few hard gates locally before it commits and tags, so a release that can't ship is never
tagged. Beyond the version/CHANGELOG checks and `oxfmt --ci`, four are worth knowing:

- **Only a commit with a fully green CI run ships.** `scripts/release-ci-gate.sh` requires HEAD to be `origin/main` (CI
  never saw an unpushed commit) and a successful `CI` run on that exact commit, started via `workflow_dispatch` with
  `run_all`. ❗ A push run doesn't count: CI's change detection skips unchanged lanes, so a docs-only HEAD reads green
  while the commit before it broke Rust. The gate reuses a full run already on the commit, starts one otherwise, and
  waits (~25 min), so `/release` pushes and starts it early to overlap it with changelog drafting. A flaky red run:
  `gh run rerun <id> --failed`, then re-run the release. The release commit itself (version bumps, CHANGELOG) lands
  after the gate; the `oxfmt --ci` gate and the post-tag CI run cover it.
  - **How a run proves it was full:** the runs API carries no inputs, so `ci.yml` has a `Full run (run_all)` job that
    runs only with `run_all` on (`if: inputs.run_all`, no needs). Both gates count a run only when that job succeeded,
    so a dispatch with `run_all` off never passes. They match the job by name: renaming it means updating both.
  - **The release workflow enforces it again server-side** (the `ci-gate` job, which `build` and `sbom` wait on), so a
    tag pushed by hand can't skip it. It looks for a successful full run of `ci.yml` on the tagged commit, or on its
    parent when the tagged commit is the `chore(release): vX.Y.Z` commit `release.sh` makes after the local gate. It
    doesn't wait: the local gate already did. If it fails because the run was still going, wait for it, then "Re-run
    failed jobs".
  - **CodeQL gates the release too**, in both places: a green `codeql.yml` run on the commit, and no open high or
    critical CodeQL alert on `main` (`scripts/release-codeql-gate.sh`, `docs/tooling/ci.md` § CodeQL). The bypass below
    skips it as well.
  - **Emergency bypass** for a hotfix while CI is red for reasons outside the repo: `RELEASE_SKIP_CI_GATE=1` locally,
    AND the repository variable `RELEASE_SKIP_CI_GATE_TAG` set to the exact tag (like `v0.51.1`) before pushing it. Same
    shape as `RELEASE_REPUBLISH_TAG`: it unblocks only the tag written in it. Clear it once the run finishes.

- **Changelog refs must survive the script's own rebase.** The script's `git pull --rebase` rewrites every unpushed hash
  whenever `origin/main` moved, so right after it, `pnpm check changelog-links --fresh` fails the release on any ref no
  longer reachable from HEAD. Fix: re-derive the stranded refs from the rebased commits, then re-run. The `/release`
  skill pulls before drafting, so this normally stays quiet. Why `--fresh`: `scripts/check/checks/DETAILS.md` §
  "CHANGELOG commit refs".
- **Visual baselines are auto-refreshed (Docker required).** After the CHANGELOG/roadmap/version are finalized, the
  script runs `apps/website/scripts/update-visual-baselines.sh`, which re-shoots any stale website baseline in a pinned
  Playwright container and folds the result into the release commit. Docker must be running; a stopped Docker aborts the
  release before tagging. The baselines no longer include `/features`, so release-prep copy shouldn't move any of them;
  the call stays as a cheap guard. Mechanism: `apps/website/DETAILS.md` § Visual baselines.
- **No stale translations may ship.** A non-`en` translation whose stored `@key.sourceHash` no longer matches the
  current English value is STALE (it renders text translated from a sentence that no longer exists). The
  `desktop-i18n-stale` check is warn-only in normal `pnpm check` (a maintenance signal, not a daily-dev build breaker),
  but the release script escalates it to a build-failing ERROR by running
  `CMDR_I18N_STALE_STRICT=1 pnpm check i18n-stale`. With `set -e`, a stale finding aborts the release before tagging, so
  the re-translation lands first. Fix: re-translate the changed keys and refresh `@key.sourceHash` (and re-review), then
  re-run the release. English-only today, so this is a clean no-op until a real locale exists. Mechanism and schema:
  `apps/desktop/src/lib/intl/messages/DETAILS.md` § `@key` metadata schema.

## The settings-defaults stamp

Right after bumping `package.json`, `scripts/release.sh` runs `gen-analytics-defaults.ts --promote <version>`. That
records the release's settings defaults in the analytics manifest (an entry only when a default moved) and stamps
`promotedThrough` either way. `settings-defaults` fails while the stamp is behind `package.json`. If a release ever
skips the step, run the promote on a tree whose settings match that release's tag: a later working tree would record
defaults the release never shipped. Mechanism: `apps/desktop/scripts/DETAILS.md` § "The defaults manifest".

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

**Not needed today**: `runs-on` is `macos-latest`, so the build happens on GitHub's hardware, this Mac can sleep, and
the laptop can close with no effect on the release. Everything below applies only if a self-hosted runner is ever set up
again.

A self-hosted runner would live on this Mac. If the machine sleeps (even briefly, or just the display), GitHub Actions
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
- `Cmdr_<version>_<arch>.app.tar.gz` plus its `.sig`: the updater payload and its minisign signature, made in CI or on
  the laptop (§ Who signs the update archives).
- `latest.json`, whose copy in `apps/website/public/latest.json` (committed by the publish job) is what
  `getcmdr.com/latest.json` serves.
- `checksums.txt`: one `shasum -a 256` line per DMG (plus the pkg when there is one), the only way a person who
  downloaded from the website (or from an aggregator listing) can check what they got. The Homebrew cask carries its own
  `sha256` and the updater verifies a minisign signature, so this covers the one install route that had nothing. The
  publish job builds it by downloading the DMGs back FROM the release, ❌ never from the runner's `target/`, so it
  describes what users actually receive, and it fails the job unless all three lines are there. First published with
  v0.46.0 (uploaded by hand after the fact; every release from v0.47.0 on gets it from the workflow). The website links
  it as `SHA-256 checksums` in the download card through `getcmdr.com/download/latest/checksums`, an api-server redirect
  beside the per-arch DMG ones that resolves `latest` the same way; ❌ it writes no `downloads` row, since a checksum
  fetch is not an app download and would inflate the per-version counts.
- `Cmdr_<version>_universal.pkg`: the signed, notarized installer package for MDM deployment, once the installer
  certificate is set up (§ The installer package). Universal only: an MDM pushes one package to every Mac.
- Three CycloneDX SBOMs, uploaded by the `attest` job: `Cmdr_<version>_aarch64.rust.cdx.json` and
  `Cmdr_<version>_x64.rust.cdx.json` (the Rust crate graph per target triple) and `Cmdr_<version>_frontend.cdx.json`
  (the npm packages whose code is in the built frontend). Details in § Provenance and SBOM attestations.

Every asset also carries a signed SLSA build provenance attestation, stored on the repo, and the same provenance is on
the release as `Cmdr_<version>.intoto.jsonl` (same section).

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

## Provenance and SBOM attestations

Two jobs in `release-pipeline.yml`, beside the build-and-publish chain:

- **`sbom`** (`needs: [guard, ci-gate]`, read-only token, no OIDC) runs beside the macOS builds. It generates the three
  SBOMs: `cargo cyclonedx` (version pinned in the workflow, CycloneDX 1.5) on `apps/desktop/src-tauri/Cargo.toml` with
  default features, once per target triple, and a frontend build with `CMDR_FRONTEND_SBOM=1` for the npm side, which
  lists the packages the bundler actually put in the app (`apps/desktop/scripts/vite-frontend-sbom.ts`; `package.json`
  can't say, since Svelte ships from devDependencies). It fails if a lockfile moved while resolving, or if an SBOM isn't
  CycloneDX, doesn't name this version as its root, or lists too few components (100 for Rust, 30 for the frontend). It
  hands them on as the `sboms` workflow artifact.
- **`attest`** (`needs: [publish, sbom]`, holds `id-token: write` and `attestations: write`) downloads every asset back
  from the published release, uploads the SBOMs to it, and runs `actions/attest`: one SLSA provenance attestation
  covering every asset (SBOMs included), then one SBOM attestation per SBOM, bound to the builds it describes (each Rust
  SBOM to its arch's DMG and tarball plus the universal ones and the pkg; the frontend SBOM to all of them). It also
  uploads the provenance's Sigstore bundle as `Cmdr_<version>.intoto.jsonl`: one file whose subjects are all the assets,
  for offline verification and for OpenSSF Scorecard's Signed-Releases check, which scores 10/10 only with an
  `*.intoto.jsonl` asset. A later run of the job replaces it (in `local` mode the dispatch's bundle, which adds
  `latest.json` and `checksums.txt`, wins). The repo-stored attestation stays what `gh attestation verify` reads.

Why it's shaped this way:

- **Attest the bytes as published.** Same rule as `checksums.txt`: the digests come from the release, ❌ never from
  `target/`, so they're what users get after signing, notarization, stapling, and the upload.
- **In `ci` mode nothing waits on `attest`.** It runs after `publish` has already shipped `latest.json`, so a Sigstore
  or attestation-API outage leaves the release exactly as it was before these jobs existed. `attest` also runs when
  `sbom` failed (its SBOM steps skip), so provenance never waits on SBOM tooling. In `local` mode the laptop waits on
  the tag push's `attest`, since it signs only what that provenance covers (§ Attestations in `local` mode).
- **The job that can sign runs no third-party code.** `cargo install` and `pnpm install` live in `sbom`, which has no
  OIDC grant; `attest` only downloads and calls `actions/attest`.
- **Every job lives in a reusable workflow.** `release.yml` is only the `v*` tag trigger: one job that calls
  `release-pipeline.yml` with the union of the jobs' permissions and `secrets: inherit`. So the attestation's signing
  certificate names `release-pipeline.yml` as the signer workflow, which is GitHub's recipe for SLSA Build Level 3
  (verified against GitHub's "Using artifact attestations and reusable workflows to achieve SLSA v1 Build Level 3",
  2026-10-05). ❌ Don't move jobs back into `release.yml`. Inside the call, `github.*` and `vars.*` are the caller's
  (the tag, this repo's variables), and job names show as `Release / <job>`.
- **The public claim is still Level 2** until a real release passes the `--signer-workflow` check below. The first
  release built through `release-pipeline.yml` hasn't happened yet.

Checking a release (the `/release` command does this after the run):

```bash
gh attestation verify Cmdr_X.Y.Z_aarch64.dmg --repo vdavid/cmdr \
  --signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml
gh attestation verify Cmdr_X.Y.Z_aarch64.dmg --repo vdavid/cmdr --predicate-type https://cyclonedx.org/bom
```

The first checks provenance and that the reusable workflow signed it, the second the SBOM binding. Earlier releases were
signed by `release.yml` itself, so check those with `--signer-workflow` pointing at `release.yml`. The public
instruction on `/trust` is the shorter `gh attestation verify <file> --repo vdavid/cmdr`.

**Once the first check passes on a real release**, move the public claim to Level 3: the SLSA mentions on `/trust`
(`apps/website/src/pages/trust.astro`, linking `slsa.dev/spec/v1.0/levels#build-l2`) and `/trust/development`
(`apps/website/src/pages/trust/development.astro`), then this paragraph and the bullet above.

## The installer package

MDMs deploy software as flat, signed, notarized `.pkg` installers, so each release can carry
`Cmdr_<version>_universal.pkg` beside the DMGs. The `pkg` job in `release-pipeline.yml` builds it; the packaging itself
lives in `scripts/build-pkg.sh`, which also runs locally. Admin-facing side (deploying it, the Full Disk Access
profile): `/trust#mdm-deploy`.

**How it's built.** The job downloads the universal DMG from the release, as uploaded, takes `Cmdr.app` out of it, and
checks it's stapled and Gatekeeper-accepted. So the pkg wraps exactly the app people download. `build-pkg.sh` then:

- runs `pkgbuild` with a component plist: **not relocatable** (otherwise Installer "upgrades" whatever copy with the
  same bundle id it finds, in Downloads or a dev build, instead of installing to `/Applications`), **not
  version-checked** (an admin can roll back by pushing an older pkg), upgrade in place, strict bundle id;
- installs to `/Applications`, owned `root:wheel` (`--ownership recommended`), with **no scripts**;
- wraps it with `productbuild` in a distribution that allows the system domain only, the app's own architectures, and
  macOS 10.15 and up; signs it with `Developer ID Installer: Rymdskottkarra AB (83H6YAQMNP)` (the application identity
  in `tauri.conf.json` with the kind swapped, so the team has one source);
- notarizes it (`notarytool`, the same API key as the DMGs), staples it, and checks it: `pkgutil --check-signature`,
  `spctl --assess --type install`, and the expanded package (install location, `relocatable="false"`, every Bom entry
  `0/0`, the binary in the payload).

Apple's guidance (Xcode docs, "Packaging Mac software for distribution", read 2026-10-07): sign with a Developer ID
Installer identity, notarize the outermost container, staple it. The app inside is already notarized from the DMG build,
which does no harm.

**Skips until the certificate exists.** Without `APPLE_INSTALLER_CERTIFICATE` in the `release` environment, the job logs
a notice and ends green, and the release ships without a pkg. `publish` and `attest` wait for the job (so
`checksums.txt` and the provenance cover the pkg) but don't need it to succeed: in `ci` mode a red `pkg` job never holds
up the release. In `local` mode it does, since `release-finish.sh` waits for a fully green tag-push run: re-run the
failed job, or, if the pkg can't be fixed now, delete `APPLE_INSTALLER_CERTIFICATE` from the environment and re-run it,
which makes it skip.

**Signed in CI in both modes.** `RELEASE_UPDATE_SIGNING=local` keeps the updater key off GitHub because whoever holds it
can ship an update to every installed Cmdr, for good. The installer certificate can't: a pkg reaches a Mac only when an
admin or a user runs it, and the certificate is revocable like the application one that already signs in CI. Moving it
to the laptop would add a second signing round trip to every release for no reduction in reach.

**The updater on a pkg install.** The app lands in `/Applications` owned by root, so the in-app updater can't write it
and asks for an administrator password (`apps/desktop/src-tauri/src/updater/DETAILS.md`, the `osascript` escalation).
Decided 2026-10-07: keep `root:wheel` and no postinstall script. That's the norm for `/Applications` and what a security
review expects, and a `chown` to the console user would let any process running as that user rewrite an app that may
hold Full Disk Access. Managed Macs whose users aren't admins should set `DisableUpdates` (`/trust#mdm`) and push each
new pkg from the MDM; the trust page says so.

**Testing the packaging locally**: `./scripts/build-pkg.sh` packages `/Applications/Cmdr.app` unsigned into a temp dir
(or `--app <path> --out <dir>`). It never installs: `installer` needs root, and a test install would replace the real
`/Applications/Cmdr.app`. So it reads the package instead (above). Expect `write: Permission denied` lines from
`pkgbuild` and a warning about `._` entries when an agent runs it: the agent's process stamps `com.apple.provenance` on
every file it copies, which nothing can strip (verified on macOS 27.0, 2026-10-07, building from 0.50.0).

**Not verified yet**: a signed, notarized build (no certificate), and an install through a real MDM. Both are open on
`vdavid/cmdr#118`.

### Setting up the installer certificate

1. In Keychain Access, select the "Developer ID Installer: Rymdskottkarra AB (83H6YAQMNP)" certificate with its private
   key (My Certificates), then File › Export Items… as `developer-id-installer.p12` with a new password. Same steps as
   the application certificate: `docs/guides/apple-signing-and-notarization.md` § 1.5.
2. Add both to the `release` environment, with the values never on screen or in shell history:
   `base64 -i developer-id-installer.p12 | gh secret set APPLE_INSTALLER_CERTIFICATE --env release -R vdavid/cmdr`, then
   `gh secret set APPLE_INSTALLER_CERTIFICATE_PASSWORD --env release -R vdavid/cmdr` (it prompts).
3. Back up the `.p12` and its password where the other signing keys live (vault note
   `projects/Cmdr/workflow/Cmdr signing keys.md`), then delete the file.
4. Optional local proof before a release:
   `./scripts/build-pkg.sh --sign "Developer ID Installer: Rymdskottkarra AB (83H6YAQMNP)"` signs from the login
   keychain and runs `pkgutil --check-signature`.
5. The next release attaches the pkg. Then update `/trust` (the `DevTodo` under "Deploying Cmdr") and this section's
   "Not verified yet".

### Renewing the Developer ID Installer certificate

David's calendar reminds him every 10 months. Steps 1–3 and 5–6 are his (portal and Bitwarden), step 4 is an agent's.

1. Keychain Access › Certificate Assistant › Request a Certificate From a Certificate Authority: his email, a common
   name, "Saved to disk".
2. At `developer.apple.com/account/resources/certificates/add`, choose **Developer ID Installer** and the **G2 Sub-CA**,
   upload the CSR, download the `.cer`, and double-click it to install.
3. ❗ The G1 trap: always pick G2 in the portal. A certificate made through Xcode on 2026-10-07 chained to the old
   "Developer ID Certification Authority" (G1) and expires 2027-02-01.
4. Export and set the secrets, keeping the password out of shell history (hand it to David once, for step 5):
   - `security export -k login.keychain-db -t identities -f pkcs12 -P "$PW" -o all.p12` exports every identity (with
     `PW=$(openssl rand -base64 24)` in the same shell).
   - Pull out the one identity: `openssl pkcs12 -in all.p12 -passin env:PW -nodes`, keep the "Developer ID Installer"
     certificate and the key whose `localKeyID` matches it, then `openssl pkcs12 -export -legacy` them into
     `developer-id-installer.p12` with `-passout env:PW` (without `-legacy`, macOS `security import` in CI may reject an
     OpenSSL 3 file). Check with `openssl pkcs12 -in developer-id-installer.p12 -passin env:PW -info -noout -legacy`.
   - Set both:
     `base64 -i developer-id-installer.p12 | gh secret set APPLE_INSTALLER_CERTIFICATE --env release -R vdavid/cmdr`,
     then `printf %s "$PW" | gh secret set APPLE_INSTALLER_CERTIFICATE_PASSWORD --env release -R vdavid/cmdr`. Delete
     `all.p12` and the PEM files.
5. David stores the `.p12` and its password in Bitwarden, then deletes the file.
6. Revoke the previous certificate in the portal.

## How updates work

- App checks `https://getcmdr.com/latest.json` on start and every 60 min
- If newer version found → downloads silently → shows "Restart to update" toast
- Signatures verified with public key embedded in app

## The tag guard, and rolling back

Pushing a `v*` tag runs the whole Release workflow, and `publish` rewrites `latest.json` for whatever tag fired it. It
has no idea which version that is, so re-pushing an old tag points the entire install base at an old build. The `guard`
job first verifies the annotated tag's SSH signature against `.github/release-signers`, then requires the tag's version
to be strictly greater than the version `apps/website/public/latest.json` currently advertises on `main`. ❗ On `main`,
never in the checkout: the tagged commit always predates its own `publish` commit, so its copy names the previous
release and lets an old tag through. The release script creates that signed tag with the configured Git signing key. A
missing key or declined passphrase aborts locally, and an unsigned or differently signed tag aborts in CI. The guard
runs in seconds and blocks `build`, so a bad push never reaches the three 90-minute macOS jobs and never overwrites
assets on an old release.

The signing key is replaceable. To rotate it, commit the new public key in `.github/release-signers` before cutting the
first release signed by its private half. An older tag keeps the signer file from its own tagged commit, so replacing
the current public key does not invalidate published releases or prevent their workflow jobs from being re-run.

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
git tag -s -m "Cmdr v0.x.x" v0.x.x     # recreate signed tag
git push origin main --tags            # push again
```

The `guard` job doesn't get in the way here: the failed run never published, so `latest.json` still names the previous
version and this tag is still ahead of it. It only refuses once the manifest already advertises this same version, at
which point the retry would be a rebuild of something users have.

### Draft release left on GitHub after failed build

In `ci` mode, go to GitHub → Releases → delete the draft manually before retrying. In `local` mode the draft is the
point: a retry of the same tag reuses it (the `draft` job finds it, and the builds replace same-named assets). Only
delete one when the `draft` job reports more than one release on the tag.

### `release-finish.sh` stopped

Fix the cause, then run it again; it picks up where the release stands.

- **"refusing to sign … its build provenance didn't verify"**: the tag push's `attest` job failed or didn't run, or the
  archive on the draft isn't the one the pipeline built at the tag. Check the `attest` job first (re-run failed jobs if
  it was an outage). ❌ Never sign around it, and never pass `-signer-workflow` outside a dry run (the tool refuses):
  that check is the whole point of signing locally.
- **"the build run ended failure"**: re-run its failed jobs, `gh run rerun <id> --failed`, then run this again.
- **"has N releases on GitHub"**: duplicate drafts on one tag. Keep the one with the assets, delete the rest.
- **"the signature for … doesn't verify against the app's public key"**: the key in sops isn't the app's (key ID
  `A2601F36BB168C0A`). Compare with the Bitwarden copy (vault note `projects/Cmdr/workflow/Cmdr signing keys.md`). The
  finishing run's `publish` job runs the same check (`release-finish.sh -verify-dir`) before it publishes the draft, so
  a dispatch with missing, empty, or foreign signatures fails there and nothing goes public.
- **"the finishing run failed again"**: it already re-ran the failed jobs once. Read the run; § Publish job failed but
  builds succeeded applies, as does § The attest or sbom job failed. ❌ Don't dispatch a fresh run by hand once
  `publish` has committed the manifest: the guard refuses it. Re-run the failed jobs instead.

### Apple notarization is slow (builds time out at 30 min)

Apple's notarization can take anywhere from minutes to 20+ hours. If the build job times out waiting for notarization,
the publish job won't run, with no broken state.

To check notarization status manually:

```bash
KEY=$(mktemp -t AuthKey_C9VUN857DD)
trap 'rm -f "$KEY"' EXIT
secret APPLE_API_KEY_BASE64 | base64 -d > "$KEY"
xcrun notarytool info <SUBMISSION_ID> \
  --key "$KEY" \
  --key-id C9VUN857DD \
  --issuer "$(secret APPLE_API_ISSUER)"
```

The credentials live in the infra sops store, so nothing needs to sit unencrypted on disk. See
`docs/guides/apple-signing-and-notarization.md` for the full manual submit-and-staple path.

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

- **Missing signatures**: check that all 3 build jobs (in `local` mode, `release-finish.sh`) uploaded their `.sig`
  files. The publish job validates this upfront and fails fast with a clear message. It looks for
  `Cmdr_<version>_<arch>.app.tar.gz.sig`, so a `tauri-action` bump that changes bundle naming lands here first.
- **`latest.json` points at an asset that isn't on the release**: same cause, caught by the assertion the job runs
  before uploading the manifest. Compare `gh release view <tag> --json assets` against the names the workflow builds,
  and fix the workflow rather than the release.
- **Git push failed**: another commit was pushed to main between checkout and push. Re-run the publish job; it does
  `git pull --rebase` to handle this, but if the rebase itself conflicts (someone else edited `latest.json`), it needs
  manual resolution.
- **Website deploy webhook failed**: re-trigger manually by pushing any commit to main, or SSH into the server and run
  the deploy script.

### The attest or sbom job failed

The release already shipped, so nothing is urgent: users get the same build as before these jobs existed, just without
attestations (or without SBOMs). Read the failure, then use "Re-run failed jobs". That re-runs only the failed jobs and
the ones depending on them, so the guard, the builds, and `publish` stay as they were. ❌ Don't re-run all jobs: the
guard refuses, because the manifest already names this version.

- **`sbom` failed**: `attest` still ran and attested the other assets. Re-running `sbom` re-runs `attest` too, which
  then uploads the SBOMs and re-attests everything (a second provenance attestation for the same digests is harmless).
- **`attest` failed on the asset check**: an expected DMG or `.app.tar.gz` isn't on the release, which `publish` should
  have caught first. Compare `gh release view <tag> --json assets` against the names the job builds.
- **`attest` failed in `actions/attest`**: usually Sigstore or the attestation API. Retry later.
- **`attest` failed with "Unsupported SBOM format"**: the action accepts CycloneDX only with `bomFormat`, `specVersion`,
  AND `serialNumber`, and the last is optional in the spec, so neither generator writes it. The `sbom` job stamps a
  UUIDv5 serial and its validation requires one. ❗ A re-run can't repair a release that shipped without it: a re-run
  uses the workflow as it was at the tag, so the SBOMs stay published and unattested until the next release (v0.47.0
  shipped this way).

### `codesign` fails with `errSecInternalComponent` (and `gh` stops working after a release)

`errSecInternalComponent` from `codesign` means the signing key can't be resolved or accessed cleanly. Three ways this
happened on the self-hosted runner:

- **The llama-server dylib signing in `beforeBuildCommand` leaned on the login keychain.** tauri-action's bundler sets
  up its own signing keychain from `APPLE_CERTIFICATE`, but only at bundling time; `download-llama-server.go` signs the
  bundled dylibs before that. The runner's launchd service runs with `SessionCreate=true` (GitHub's `svc.sh` plist), so
  its jobs live in their own security session where the login keychain's private key isn't usable (the exact same
  `codesign` command works from a GUI shell), and every matrix job failed ~30 s in. A runner-service restart doesn't
  help. The fix in `release-pipeline.yml` ("Set up llama-server signing keychain") imports the cert into a dedicated
  keychain that the Go script targets explicitly via `codesign --keychain` (`LLAMA_SIGN_KEYCHAIN`). The keychain must
  ALSO be in the user keychain search list: `--keychain` alone fails with the same `errSecInternalComponent` for a
  keychain outside the search list (verified empirically on this runner). The explicit `--keychain` is what keeps the
  login keychain's copy of the identity from making resolution ambiguous; the "Restore keychain search list" cleanup
  step resets the list afterwards.

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

Prevention: step 4 of `.claude/commands/release.md` reads the resolved path's `auth_value` right after the CHANGELOG
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
