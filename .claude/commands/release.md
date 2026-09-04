Prepare a release based on docs/guides/releasing.md.

1. Prerequisite: Run `gh secret list` and verify that `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` both exist. If either is missing, warn the user and stop.
2. Update @CHANGELOG.md based on git commits since last release.
   - Commits have title + body. Read all!
   - You can reference multiple commits for changelog items if needed.
   - List major but non-app changes in a "Non-app" section.
   - **Reference commits as bare hashes in a trailing group**: `- Some change (b626d7a4, 2d41cc14)`. Never write a
     markdown link; the website and the What's new popup linkify (or strip) the hashes themselves, and the
     `changelog-links` check rejects a `…/commit/<sha>` URL.
   - **Get commit SHAs via `git log --format='%h' --abbrev=8`**. Never extend a 7-char prefix from `git log --oneline`
     by guessing the next character. The committed changelog convention is 8 chars; let git produce them. The
     `changelog-links` check enforces that length exactly, and rejects fabricated SHAs, aborting the release either way.
   - **Add a `## [Unreleased]` heading** right after the format preamble (before the first versioned section), then put
     entries under it. The release script replaces this heading with the versioned one. The committed changelog has no
     `[Unreleased]` section between releases. You're creating it fresh each time.

   ### Audience: who reads this

   One file, two audiences:
   - **Primary: Cmdr users.** The prose lead and the Added / Changed / Fixed / Security sections become the GitHub
     release notes and the in-app "What's new" popup, rendered with commit hashes stripped and Non-app dropped. Write
     them so every entry works standalone, in plain English, with zero internals.
   - **Secondary: David and agents tracing changes.** Served by the commit hashes and the Non-app section; Non-app is
     the only place internals (tooling, refactors, infra, website) belong.

   ### Scope: no inflated fixes

   **A changelog is the diff between the last release and this one, not a diary of the work.** So a bug only earns a
   Fixed line if a USER COULD HAVE HIT IT: the defect has to have existed in the previous release's tree. A bug
   introduced and fixed inside this same release window doesn't qualify. `git cat-file -e v<prev>:<path>` helps when in
   doubt.

   ### Style: plain-sentence, dense, impact-focused
   - **Write a 1–2 sentence plain-prose lead** directly under the `## [Unreleased]` heading, before `### Added`: what
     this release means for users, naming the one to three highlights. No links, no bullets. It opens the release notes
     and the What's new popup; see the recent releases for examples.
   - **File each entry where a user would look for it.** A fix to previously shipped behavior is Fixed even when this
     release also adds a feature in that area (but see § Scope: no inflated fixes); perf and behavior tweaks are
     Changed; pure internals go to Non-app. Only Added / Changed / Fixed / Security / Non-app; never invent sections
     like "Improved".
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
   >   out, and reset. Everything scales: row height, icons, column widths, breadcrumbs, viewer (3 SHAs)

   **Do**:

   > - Add dynamic text size slider in Settings (75–150%, ⌘+/⌘-/⌘0 shortcuts) (3 SHAs)

   **Don't**:

   > - Brief network blips no longer kick you out of the folder; only a real not-found triggers eviction (48ac9bf8)

   **Do**:

   > - Fix temp network issues kicking users out of folders (48ac9bf8)

   **Don't**:

   > - Friendly errors for the git browser: damaged repos, orphaned worktrees, shallow-boundary commits, locked indexes
   >   get plain-language explanations and a next step (19d5b075, af64689f)

   **Do**:

   > - Add friendly errors for git browser (19d5b075, af64689f)

   **Don't** (multi-sentence narration; a real past draft):

   > - Folders always merge on copy and move. A folder landing on a same-named folder now blends into it instead of
   >   asking you to overwrite, skip, or rename the whole folder. Your conflict choice (skip, overwrite, or rename)
   >   applies to the clashing files inside, so dest-only files always survive the merge (2 SHAs)

   **Do**:

   > - Folders always merge on copy and move: your conflict choice (skip, overwrite, or rename) applies to the clashing
   >   files inside, and dest-only files survive (2 SHAs)

   **Don't** (trailing benefit clause; a real past draft):

   > - Add per-feature stability badges (ALPHA, BETA) in the app and a Feature status page on the website, so you always
   >   know how solid each feature is (219549db)

   **Do**:

   > - Add stability badges (ALPHA, BETA) in the app and a feature status page on the website (219549db)

   **Keep long when warranted** (true tentpole launches like Linux alpha, with many real commits and a big story):

   > - Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
   >   inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native
   >   file icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no
   >   system keyring, distro-specific install hints, USB permission handling (13 SHAs)

   #### Self-edit pass (mandatory)

   After drafting and before presenting the draft, re-read every entry and:
   - **The changelog now ships inside the app** (the in-app "What's new" popup renders these exact sections), so this
     pass is also a UI-copy review: hold every entry to the `docs/style-guide.md` writing bar, not just changelog
     conventions.
   - **Think about every Fixed and Security entry against § Scope: no inflated fixes.** For each one, name the release
     the bug shipped in. If that's this release, fold the hash into the feature's Added entry and delete the line. This
     is the easiest pass to skip and the one that inflates a release most.
   - Cut any entry over ~20 words unless it's a genuine tentpole.
   - Merge or delete any second sentence.
   - Delete any trailing ", so [benefit]" clause.
   - Check that no two entries share commit SHAs or describe the same change from two angles; merge them. (A real past
     draft had a Fixed entry whose SHAs were a strict subset of an Added entry's.)
   - Strip internal symbol names, file paths, and enum variants that survived the first pass.

3. **Only if `release.yml`'s `build.runs-on` is the self-hosted runner** (it is `macos-latest` today, so normally SKIP
   this step): check the runner's Finder Automation permission so `bundle_dmg.sh` doesn't hang for ~2 minutes per matrix
   job. Run it AFTER presenting the CHANGELOG draft for review (the user is at the keyboard anyway). See
   `docs/guides/releasing.md` § "Which runner builds the release" and § "`bundle_dmg.sh` hangs ~2 minutes then fails on
   every matrix job" for why this is needed, the `auth_value` codes, and how to recover.

   ❌ **Resolve the REAL path, never `externals/`.** That's a symlink into `externals.<version>/`, tccd keys its rows on
   the resolved path, and the symlink path carries a stale `2` row of its own from earlier grants. Checking the symlink
   reports "already allowed" exactly when the runner has just auto-updated, which is the one case this step exists to
   catch.

   ```bash
   NODE=$(readlink -f ~/actions-runner/externals/node20/bin/node 2>/dev/null)
   if [ -x "$NODE" ]; then
     CURRENT=$(sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
       "SELECT auth_value FROM access WHERE client='$NODE' AND service='kTCCServiceAppleEvents' AND indirect_object_identifier='com.apple.finder';")
     echo "auth_value='${CURRENT:-<no row>}' for $NODE"
   fi
   ```

   Read the answer as: `2` is allowed, carry on. `0` is **denied and blocks the release** — no prompt will ever fire
   again, so fix it before tagging (recovery in the guide; it needs the user at the GUI). Empty or `1` means the next
   build prompts, so tell the user to stay at the keyboard and click Allow when it appears.

   ❌ **Don't try to pre-fire the prompt from your own shell.** An `osascript` run from an agent/Terminal shell is
   attributed to that shell's already-granted responsible process, so it succeeds without asking about node and writes
   nothing useful. Only the runner's own launchd session (`SessionCreate=true`) makes node the responsible process.

4. Apply the roadmap and feature-status updates (edit the files, don't just advise; the user reviews before committing).
   - **Roadmap** (@apps/website/src/pages/roadmap.astro): add a dated milestone (with a date!) for each major
     development this release, and tick off / remove any "coming soon" item that just shipped. Match the existing
     curation: milestones only, not every release. Group under the right month heading (add a new `<h3>` when the month
     rolls over) and not the release date but the actual main development date based on the commits.
   - **Feature status** (`feature-status.json` at the repo root, the single source of truth behind the `/features` page
     and the in-app badges): review every feature against what shipped. Flip `planned` → `alpha` for a feature that just
     launched, graduate `alpha` → `beta` → `stable` as one matures, and refresh any note the release made stale. Keep
     notes to one honest line, website voice (no "I"/"we"). Schema and consumers: @docs/feature-status.md. Graduating
     `search` or `select-files` out of `alpha` also means updating the pinned assertion in
     `apps/desktop/src/lib/feature-status.test.ts`. Present the diff for review.
5. Based on the changes, advise what the next version should be (patch: bug fixes, minor: new features, major: major
   launches), and give the user the `./scripts/release.sh x.x.x` command to run.
6. **Offer to run the release script** for the user. Wait for confirmation before running.
7. **Push immediately** with `git push origin main --tags` IFF the release script completed cleanly. Else: stop and ask.
8. **After pushing**, confirm the build started. Wait ~30 seconds, then run `gh run view <release-run-id> --json jobs`
   and check the `Build (...)` jobs. On GitHub-hosted runners (the current setup) all three should be `in_progress`
   together, because they run in parallel.
   - **Self-hosted only**: exactly one goes `in_progress` and the other two stay `queued`, which is normal (one machine,
     serialized). If all three are still `queued` after ~30 s the runner is down: confirm with
     `launchctl list | grep cmdr` (look for `actions.runner.vdavid-cmdr.*`) and restart with
     `cd ~/actions-runner && ./svc.sh start` (fall back to `launchctl bootout` + `bootstrap` if `svc.sh` errors with
     "Load failed: 5: Input/output error"). Queued jobs pick up automatically once the runner reports in; no re-trigger
     or re-tag needed.
9. **Self-hosted only, so normally SKIP: arm `caffeinate`** so the Mac can't sleep mid-build (a display or system sleep
   drops the self-hosted runner connection and fails every in-flight matrix job). While the build runs on GitHub's
   hardware nothing local matters, so don't arm it. Follow the check/arm/disarm procedure in `docs/guides/releasing.md`
   § "Keep the Mac awake during the build": check `pgrep -lf 'caffeinate -dimsu'` first, arm a background
   `caffeinate -dimsu` only if none is running, disarm it once the workflow reports `completed` (and only if you armed
   it), and re-arm before a re-run of failed jobs if none is running.
10. **Monitor the CI build**:

- On GitHub-hosted runners, tell the user their laptop is free: they can close it or sleep the Mac, the build is
  elsewhere. **Self-hosted only**: remind them NOT to close the laptop for ~15 minutes.
- Poll `gh run view` every few minutes in the background and report progress (which jobs are done, which are still
  running). Self-hosted, warm cache, sequential: ~5 min each for aarch64 and x86_64, ~7 min for universal. Hosted, cold
  cache, parallel: expect appreciably longer per job, offset by them running at the same time.
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
    - Confirm the updater payload behind that manifest actually resolves, for each of the three platform keys:
      `curl -s https://getcmdr.com/latest.json | jq -r '.platforms[].url' | sort -u | xargs -I{} curl -sIL -o /dev/null -w '%{http_code} {}\n' {}`
      should print `200` for all three. The workflow spells these `.app.tar.gz` names out by hand, so a naming change in
      `tauri-action` surfaces here and nowhere else until installs stop updating.
    - If `latest.json` still shows the old version after ~2 minutes, the deploy webhook may have failed silently. Tell
      the user; the manual fix is to re-trigger the website-deploy workflow via `workflow_dispatch` from the Actions
      tab. Don't block release success on this. The GitHub Release is what users actually download.

13. **Minor or major release? Offer to refresh the app-directory listings** (skip entirely for patches). Update
    @brand/listings/macupdate.md in place: the version number, the "Version changes" HTML rewritten from the new
    CHANGELOG section into their `<h5>` + `<ul>` format, and only the description lines the release actually made stale.
    That file is the source of truth; never retype a listing at the form. Then give David the link
    (https://member.macupdate.com/content/submit, "Modify an existing listing?" at the top) and stop: submitting is his
    call, and the download URL in the listing already always serves the current release. Full rationale:
    `docs/guides/releasing.md` § "Refreshing the app-directory listings".
