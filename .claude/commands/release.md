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

   ### Audience: who reads this

   One file, two audiences:
   - **Primary: Cmdr users.** The prose lead and the Added / Changed / Fixed / Security sections become the GitHub
     release notes and the in-app "What's new" popup, rendered with commit links stripped and Non-app dropped. Write
     them so every entry works standalone, in plain English, with zero internals.
   - **Secondary: David and agents tracing changes.** Served by the commit links and the Non-app section; Non-app is the
     only place internals (tooling, refactors, infra, website) belong.

   ### Style: plain-sentence, dense, impact-focused
   - **Write a 1–2 sentence plain-prose lead** directly under the `## [Unreleased]` heading, before `### Added`: what
     this release means for users, naming the one to three highlights. No links, no bullets. It opens the release notes
     and the What's new popup; see the recent releases for examples.
   - **File each entry where a user would look for it.** A fix is Fixed even if it shipped alongside a feature; perf and
     behavior tweaks are Changed; pure internals go to Non-app. Only Added / Changed / Fixed / Security / Non-app; never
     invent sections like "Improved".
   - **Each entry is one sentence.** No `**Bold title:** Body.`; the headline IS the entry. Most entries land under 20
     words; many under 10. Big aggregated entries for tentpole features (think Linux alpha, Git browser launch) can run
     several lines if they bundle many real commits.
   - **One sentence means one sentence.** No multi-sentence narration ("X now does Y. A thing landing on a thing now
     blends into it instead of… Your choice applies…"). If you wrote a period and kept going, merge with a colon or cut
     the rest.
   - **No trailing benefit clauses.** Delete ", so you always know…", ", so David can follow up", ", so the app never
     points you at a dead key". If the entry is written well, the benefit is implicit. Pattern to ban: ", so [reader
     benefit]" at the end of an entry.
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
   - **Calibrate on the two most recent release sections plus the bottom 160 lines.** Both are curated. Don't
     pattern-match on anything else, and never treat your own draft as calibration; verbosity compounds release over
     release that way.

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

   **Don't** (multi-sentence narration; a real past draft):

   > - Folders always merge on copy and move. A folder landing on a same-named folder now blends into it instead of
   >   asking you to overwrite, skip, or rename the whole folder. Your conflict choice (skip, overwrite, or rename)
   >   applies to the clashing files inside, so dest-only files always survive the merge ([2 SHAs]).

   **Do**:

   > - Folders always merge on copy and move: your conflict choice (skip, overwrite, or rename) applies to the clashing
   >   files inside, and dest-only files survive ([2 SHAs]).

   **Don't** (trailing benefit clause; a real past draft):

   > - Add per-feature stability badges (ALPHA, BETA) in the app and a Feature status page on the website, so you always
   >   know how solid each feature is ([219549db](...)).

   **Do**:

   > - Add stability badges (ALPHA, BETA) in the app and a feature status page on the website ([219549db](...)).

   **Keep long when warranted** (true tentpole launches like Linux alpha, with many real commits and a big story):

   > - Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
   >   inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native
   >   file icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no
   >   system keyring, distro-specific install hints, USB permission handling ([13 SHAs]).

   #### Self-edit pass (mandatory)

   After drafting and before presenting the draft, re-read every entry and:
   - Cut any entry over ~20 words unless it's a genuine tentpole.
   - Merge or delete any second sentence.
   - Delete any trailing ", so [benefit]" clause.
   - Check that no two entries share commit SHAs or describe the same change from two angles; merge them. (A real past
     draft had a Fixed entry whose SHAs were a strict subset of an Added entry's.)
   - Strip internal symbol names, file paths, and enum variants that survived the first pass.

3. **Pre-warm the runner's Finder Automation permission** so `bundle_dmg.sh` doesn't hang for ~2 minutes per matrix job.
   Run the canary AFTER presenting the CHANGELOG draft for review (the user is at the keyboard anyway). If a macOS
   dialog appears asking to allow control of Finder, tell the user to click Allow. See `docs/guides/releasing.md` §
   "`bundle_dmg.sh` hangs ~2 minutes then fails on every matrix job" for why this is needed, the `auth_value` codes, and
   how to recover if the entry is already stuck at denied.

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

4. Suggest updates to the roadmap.
   - Read @apps/website/src/pages/roadmap.astro as well. Is there anything to tick off (with a date!) or a major
     development worth mentioning?
5. Based on the changes, advise what the next version should be (patch: bug fixes, minor: new features, major: major
   launches), and give the user the `./scripts/release.sh x.x.x` command to run.
6. **Offer to run the release script** for the user. Wait for confirmation before running.
7. **Push immediately** with `git push origin main --tags` IFF the release script completed cleanly. Else: stop and ask.
8. **After pushing**, confirm the self-hosted runner picked up the build:
   - Wait ~30 seconds, then run `gh run view <release-run-id> --json jobs` and check the `Build (...)` jobs.
   - At least one `Build (...)` job should be `in_progress` (the self-hosted runner serializes the three matrix jobs, so
     the others stay `queued`, which is normal).
   - **If all three are still `queued` after ~30s, the self-hosted runner is down.** Confirm with
     `launchctl list | grep cmdr` and look for `actions.runner.vdavid-cmdr.*`. Restart with
     `cd ~/actions-runner-cmdr && ./svc.sh start` (fall back to `launchctl bootout` + `bootstrap` if `svc.sh` errors
     with "Load failed: 5: Input/output error"). Re-check after another 30 s. The queued jobs pick up automatically once
     the runner reports in. No need to re-trigger or re-tag.
9. **Then arm `caffeinate`** to prevent the Mac from sleeping during the build (a display or system sleep drops the
   self-hosted runner connection and fails every in-flight matrix job). Follow the check/arm/disarm procedure in
   `docs/guides/releasing.md` § "Keep the Mac awake during the build": check `pgrep -lf 'caffeinate -dimsu'` first, arm
   a background `caffeinate -dimsu` only if none is running, disarm it once the workflow reports `completed` (and only
   if you armed it), and re-arm before a re-run of failed jobs if none is running.
10. **Monitor the CI build**:

- Remind the user not to close their laptop for ~15 minutes while the self-hosted runner builds.
- Poll `gh run view` every few minutes in the background and report progress (which jobs are done, which are still
  running). aarch64 and x86_64 builds took about 5min 10sec each, universal takes about 7 min.
- Report when all jobs complete (success or failure). If a job fails, show the failure details, and advise how to fix.
- Suggest the user to also track the build at https://github.com/vdavid/cmdr/actions.

11. **Make the standalone CI run happen, then watch it** (the non-release `CI` workflow):
    - **First, check whether CI is disabled.** David sometimes disables it to save GHA minutes; `gh workflow list --all`
      then shows `CI` as `disabled_manually` (a `push` to main won't fire it). CI matters for a release: its
      `deploy-website` job is what publishes the roadmap, feature-status, and landing-page changes (the release workflow
      only refreshes `latest.json`), and the full check suite gives a quality signal on the release commit. If it's
      disabled, re-enable and trigger it on the release commit: `gh workflow enable CI` then
      `gh workflow run CI --ref main` (`run_all` defaults to true). Tell the user you re-enabled it, and ask whether
      they want it left enabled or disabled again after the run.
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
