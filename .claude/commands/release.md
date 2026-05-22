Prepare a release based on docs/guides/releasing.md.

1. Prerequisite: Run `gh secret list` and verify that `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` both exist. If either is missing, warn the user and stop.
2. Update @CHANGELOG.md based on git commits since last release.
   - Commits have title + body. Read all!
   - You can link multiple commits for changelog items if needed.
   - List major but non-app changes in a "Non-app" section.
   - **Get commit SHAs via `git log --format='%h' --abbrev=8`**. Never extend a 7-char prefix from `git log --oneline`
     by guessing the next character. The committed changelog convention is 8 chars; let git produce them. The
     `changelog-links` check will reject fabricated SHAs and abort the release.
   - **Add a `## [Unreleased]` heading** right after the format preamble (before the first versioned section), then put
     entries under it. The release script replaces this heading with the versioned one. The committed changelog has no
     `[Unreleased]` section between releases. You're creating it fresh each time.

   ### Style: plain-sentence, dense, impact-focused
   - **Each entry is one sentence.** No `**Bold title:** Body.`; the headline IS the entry. Most entries land under 20
     words; many under 10. Big aggregated entries for tentpole features (think Linux alpha, Git browser launch) can run
     several lines if they bundle many real commits.
   - **No em-dashes** (`—`). They are AI hallmarks. Use parens, commas, colons, or rephrase. En-dashes in ranges are OK.
     Vary the connector, don't default to `;`. Use `:` to explain or qualify, `,` for a tight list, parens for am aside,
     new sentence when two ideas don't compress. `;` is OK for other cases.
   - **Lead with a verb often.** `Add X`, `Fix X`, `Make Y`, `Drop Z`. The bottom of the file is a calibration reference
     for this; read a handful of entries before drafting.
   - **Cut aggressively.** Strip internal type names, file paths, code fragments, "why we picked X", etc. Git history
     has those. Keep impactful & interesting value details.
   - **Omit low-impact entries.** Tooling-only commits like "release script now stages oxfmt fixes" or "cleared 3 eslint
     warnings, CI is green again" don't earn a changelog line. If a non-app item has no interesting story for a reader,
     drop it.
   - **Read the bottom 160 lines of the file** to calibrate the style. These are hand-written and exemplary.

   #### Before / after examples

   **Don't**:

   > - **Dynamic text size.** New `Settings > Appearance > Text size` slider (75–150 %, default 100 %) that compounds
   >   with the macOS Accessibility text-size setting. New `View > Zoom` submenu with `⌘+` / `⌘-` / `⌘0` to zoom in,
   >   out, and reset. Everything scales: row height, icons, column widths, breadcrumbs, viewer ([3 SHAs]).

   **Do**:

   > - Add dynamic text size slider in Settings (75–150%, ⌘+/⌘-/⌘0 shortcuts) ([3 > > SHAs]).

   **Don't**:

   > - Brief network blips no longer kick you out of the folder; only a real not-found triggers eviction
   >   ([48ac9bf8](...)).

   **Do**:

   > - Fix temp network issues kicking users out of folders ([48ac9bf8](...)).

   **Don't**:

   > - Friendly errors for the git browser: damaged repos, orphaned worktrees, shallow-boundary commits, locked indexes
   >   get plain-language explanations and a next step ([19d5b075](...), [af64689f](...)).

   **Do**:

   > - Add friendly errors for git browser ([19d5b075](...), [af64689f](...)).

   **Keep long when warranted** (true tentpole launches like Linux alpha, with many real commits and a big story):

   > - Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
   >   inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native
   >   file icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no
   >   system keyring, distro-specific install hints, USB permission handling ([13 SHAs]).

3. **Pre-warm the runner's Finder Automation permission** so `bundle_dmg.sh` doesn't hang for ~2 minutes per matrix job.
   When `actions-runner` auto-updates, its bundled `node` binary lands at a new path
   (`~/actions-runner/externals.<version>/node20/bin/node`) that macOS TCC has never seen. The first `osascript` call
   from that node pops an "Allow … to control Finder" prompt; if the user isn't at the keyboard, the prompt times out
   after ~2 minutes and TCC records auth_value=0 (denied) for that node, breaking every subsequent DMG bundle.

   Run the canary AFTER presenting the CHANGELOG draft for review (the user is at the keyboard anyway). If a macOS
   dialog appears asking to allow control of Finder, tell the user to click Allow.

   ```bash
   NODE=$(readlink ~/actions-runner/externals 2>/dev/null)
   [ -n "$NODE" ] && NODE=~/actions-runner/externals/node20/bin/node
   if [ -x "$NODE" ]; then
     CURRENT=$(sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
       "SELECT auth_value FROM access WHERE client='$NODE' AND service='kTCCServiceAppleEvents' AND indirect_object_identifier='com.apple.finder';")
     if [ "$CURRENT" != "2" ]; then
       echo "Triggering Finder permission prompt for $NODE — click Allow if macOS asks."
       "$NODE" -e "require('child_process').execFileSync('/usr/bin/osascript', ['-e', 'tell application \"Finder\" to return name of startup disk'], { stdio: 'inherit' })" || echo "Canary failed; user may have denied or TCC entry stuck at 0."
     fi
   fi
   ```

   - `auth_value` codes: 0=denied, 1=ask, 2=allowed. Anything other than 2 means the next bundle_dmg will hang.
   - Don't try to fix a stuck `auth_value=0` by `UPDATE`-ing TCC.db directly. tccd re-validates each row's `csreq`
     against the live binary's signature on use, plus there's an integrity layer on Sonoma+; a hand-edited row reads
     back fine via `SELECT` but tccd treats it as untrusted and re-prompts. The only reliable path is to make the prompt
     fire, which is what the canary above does.

4. Suggest updates to the roadmap.
   - Read @apps/website/src/pages/roadmap.astro as well. Is there anything to tick off (with a date!) or a major
     development worth mentioning?
5. Based on the changes, advise what the next version should be (patch: bug fixes, minor: new features, major: major
   launches), and give the user the `./scripts/release.sh x.x.x` command to run.
6. **Offer to run the release script** for the user. Wait for confirmation before running.
7. **Offer to push** with `git push origin main --tags`. Wait for confirmation before pushing.
8. **After pushing**, confirm the self-hosted runner picked up the build:
   - Wait ~30 seconds, then run `gh run view <release-run-id> --json jobs` and check the `Build (...)` jobs.
   - At least one `Build (...)` job should be `in_progress` (the self-hosted runner serializes the three matrix jobs, so
     the others stay `queued`, which is normal).
   - **If all three are still `queued` after ~30s, the self-hosted runner is down.** Confirm with
     `launchctl list | grep cmdr` and look for `actions.runner.vdavid-cmdr.*`. Restart with
     `cd ~/actions-runner-cmdr && ./svc.sh start` (fall back to `launchctl bootout` + `bootstrap` if `svc.sh` errors
     with "Load failed: 5: Input/output error"). Re-check after another 30 s. The queued jobs pick up automatically once
     the runner reports in. No need to re-trigger or re-tag.
9. **Then arm `caffeinate`** to prevent the Mac from sleeping during the build. The self-hosted runner lives on this
   Mac; any sleep (display or system) drops the runner connection and fails every in-flight matrix job with
   `The self-hosted runner lost communication with the server`. See `docs/guides/releasing.md` § "Keep the Mac awake
   during the build".
   - Run `caffeinate -dimsu` as a Bash `run_in_background` call. Capture the background task id so you can stop it.
   - Disarm it once the release workflow reports `completed` (success or failure, not just when the matrix is done).
   - If the user requests a re-run of failed jobs, re-arm caffeinate first.
10. **Monitor the CI build**:

- Remind the user not to close their laptop for ~15 minutes while the self-hosted runner builds.
- Poll `gh run view` every few minutes in the background and report progress (which jobs are done, which are still
  running). aarch64 and x86_64 builds took about 5min 10sec each, universal takes about 7 min.
- Report when all jobs complete (success or failure). If a job fails, show the failure details, and advise how to fix.
- Suggest the user to also track the build at https://github.com/vdavid/cmdr/actions.

11. **In parallel, watch the standalone CI run** (the non-release `CI` workflow that fires on the same push):
    - It's not a blocker for the release. If it goes red, fix it in the background while the release builds. Small
      things like lint regressions are common.
    - Surface the failure to the user when convenient; don't interrupt release-build progress reporting for it.
12. **After the release run succeeds, verify the public surface**:
    - `gh release view vX.Y.Z --json assets,tagName,publishedAt`: confirm the expected DMGs are attached
      (`Cmdr_X.Y.Z_aarch64.dmg`, `_x64.dmg`, `_universal.dmg`) and sizes look reasonable.
    - Wait ~30 seconds for the website auto-deploy (the release workflow commits an updated `latest.json` and fires a
      webhook), then `curl -s https://getcmdr.com/latest.json | jq -r .version` and confirm it matches `X.Y.Z`.
    - If `latest.json` still shows the old version after ~2 minutes, the deploy webhook may have failed silently. Tell
      the user; the manual fix is to re-trigger the website-deploy workflow via `workflow_dispatch` from the Actions
      tab. Don't block release success on this. The GitHub Release is what users actually download.
