# Recording the product clips

How to (re)record the short, silent, looping clips of the real app that the website plays (issue #693), and what to look
at when a run refuses. Sibling of `docs/guides/screenshots.md` (the stills): same launch layer, same "leave the machine
alone" rule.

## Run

```bash
pnpm marketing:clips --dry-run              # check the machine; launches nothing, records nothing
pnpm marketing:clips                        # every clip, dark and light, into apps/website/public/clips/
pnpm marketing:clips --only copy-rollback   # one clip; repeat or comma-separate for more
pnpm marketing:clips --out ~/Desktop/clips  # somewhere else, leaving the website untouched
pnpm marketing:clips --encode-only          # re-encode the masters already in brand/clip-masters/
pnpm marketing:clips --video                # the demo video's long-form beats instead (masters only)
```

- ❗ **Leave the machine alone while it records.** Each take brings Cmdr to the front: ScreenCaptureKit records a
  covered window fine, but WKWebView starves `requestAnimationFrame` in an occluded one, so a clip recorded behind
  another app shows frozen progress bars.
- ❗ **The reach clips need the Docker fixtures up**: `apps/desktop/test/smb-servers/start.sh minimal` and
  `apps/desktop/test/s3-servers/start.sh minimal` (§ "The reach clips").
- A full run is roughly 18 takes of ~10–30 s each plus staging, a few minutes in all, unless the drive index has to
  reconcile first (about five minutes, once; `docs/guides/screenshots.md` § "When it waits").
- A failed take fails only itself (Playwright retries it once). The run still encodes every take that recorded, then
  exits non-zero naming the skipped ones.
- The clips are drafts until David has reviewed them, like every human-facing asset.

## The clips

Slugs come from `apps/desktop/test/e2e-playwright/marketing-clips-slugs.ts`; each scene is a function in
`marketing-clips-scenes.ts`.

- `copy-rollback`: a throttled copy of 64 photos, Pause, Resume, Rollback, and the bar walks back to zero.
- `ops-queue`: the Operations window over the main window, five project folders copied one after the other.
- `multi-rename`: the name mask typed into Multi-rename (`Iceland [Y-M-D] [C:3]`), the preview filling, then Rename.
- `search` (~23 s): "invoice" typed with AI off, results landing; Settings > Provider switched to Local LLM (the
  built-in model, already downloaded into the clips instance off camera) until its server reads Running; then "Ask
  anything", "tax report from last year", Enter, and only last year's two tax files come back. Real inference, so a
  wrong answer fails the take and Playwright's retry records it again. The settings window waits behind the main window,
  centered inside its frame, from before the take: the recorder only shows windows it was given or that open after it
  starts.
- `language-switch`: Settings > Appearance > Language, the list open, ArrowDown twelve times; the picker previews each
  language as it's highlighted, so the window re-renders in each.
- `viewer` (~20 s): a photo of a real painting, then the same file as Binary (2) and Hex (3); then a Markdown README as
  Text (1), then formatted (0). Two viewer windows stacked on one frame, the README's behind: a window reaches another
  file only through a Markdown link, and a linked file opens formatted, never as text.
- `reach-network` and `reach-devices`: § "The reach clips".
- `settings`: a pan through seven Settings sections, one per group.

## The reach clips

`marketing-clips-reach.ts`, on the `--video2` fixtures dressed as a home setup (`marketing-video2-fixtures.ts`). Each
place opens in a new tab (⌘T) or is left with ⌘↑, and the tab closes again (⌘W), so both loop.

- `reach-network` (~23 s): ⌘K, `smb://home-nas.local.:11481/private`, the name "Home NAS", the account, the password,
  and the share browsed into a 5,000-photo folder; then the other pane's favorites (⌃D) on "Family photos", the
  `family-photos` bucket, browsed into an album.
- `reach-devices` (~20 s): Enter into a repo's `.git`, its commits as folders, ⌘↑ back; a restic `Backups` repo in
  `Archive/`, its password, its snapshots; then a new tab, ⌃D to a virtual "Pixel 8" over MTP, `DCIM/Camera`.
- ❗ **Places open from the favorites (⌃D), ❌ never the volume switcher (⌥F1 / ⌥F2)**: the switcher lists every volume
  the Mac has, which put David's own NAS (by its Tailscale address) and cloud drives in frame (2026-11-03). Each scene
  adds its favorite off camera (`favoriteRightPane`), named apart from the volume: a favorite named like a volume makes
  MCP's `select_volume` refuse the name as ambiguous.
- ❗ **No Servers view on camera**: its "found nearby" group lists the real servers on David's network.
- **The trailing dot in the address** works around an app bug: `build_smb_addr` strips a final `.local` for the UNC path
  and then dials the stripped name, which resolves to nothing. It also shows in the share's label ("private on
  home-nas.local.:11481"), which ignores the name typed in the sheet.

What the orchestrator stages for them (`prepareReachClips` in `apps/desktop/scripts/marketing-clips.ts`):

- **The NAS by name.** `home-nas.local` is a Bonjour host for 127.0.0.1, published for the run with `dns-sd -P` (no
  root), so no frame shows a loopback address. The S3 endpoint rides the same name. The SMB fixture's port (11481) still
  shows in the typed address: 445 on 127.0.0.1 needs root.
- **A fresh add each take.** Before launch it deletes the clips instance's saved servers (`known-shares.json`,
  `manual-servers.json`, `s3_known_places.json`), the folders the NAS share and the bucket last showed (`lastUsedPaths`
  in `app-status.json`: a place reopens there), and a stale favorite named like the bucket. Each take forgets the NAS,
  its password (`delete_smb_credentials` under the dialed `host:port`, which the saved row's forget misses), and
  unmounts its share before and after (`forgetNas`: only a mount of the fixture, matched by host and port), and backs
  out of the share and the bucket on camera before closing their tabs.
- **A locked backup each take.** The repo is `Archive/Backups` (`RESTIC_CLIPS_PARENT`): in the demo root, showing hidden
  files for `.git` would put the demo tree's marker in frame. Each take forgets its password before and after
  (`forget_saved_container_password`).
- **The phone** exists only when the app launches with `CMDR_VIRTUAL_MTP`; the scene fills its `DCIM/Camera` after
  launch (`seedPhone`), since the app reseeds the backing dir at startup.

## The image the viewer shows

"Abstract landscape with sun", acrylic on canvas, 1990, by Fons Heijnsbroek, dedicated to the public domain (CC0), so no
credit is owed anywhere it shows:
<https://commons.wikimedia.org/wiki/File:'Abstract_landscape_with_sun'_-_large_acrylic_painting_on_canvas,_painted_in_1990_by_Dutch_artist_Fons_Heijnsbroek,_free_image_of_art_in_high_resolutions.jpg>.
Committed as a 2,400 px copy of the 6,555 px original, metadata kept:
`apps/desktop/test/e2e-playwright/marketing-clips-assets/abstract-landscape-with-sun.jpg` (`ARTWORK` in
`marketing-clips-demo.ts`). A replacement must be human-made (a photo or an artwork, ❌ never AI-generated) and CC0.

## Key captions

Every key press a take makes is captioned the way KeyCastr shows it, for the website's overlay: `⌘F`, `↓`, `Enter`,
`Esc`, `⇧⌘N` (modifiers in Apple's order, ⌃⌥⇧⌘), and a typed string as one label (`invoice`). A command the scene
dispatches in place of its shortcut carries the shortcut (`F5` for `file.copy`). The scenes press keys only through
`marketing-clips-keys.ts` (`pressCaptioned`, `runCaptioned`, `typeCaptioned`), which caption onto the take that is
recording; a press off camera captions nothing. Mouse clicks aren't captioned.

The encoder writes `{slug}-{theme}.keys.json` beside each clip, the contract the website builds against:

```json
{
  "keys": [
    { "at": 1.53, "label": "⌘F" },
    { "at": 2.1, "label": "invoice" }
  ]
}
```

- `at`: seconds from the ENCODED clip's first frame (after the trim), three decimals, in press order. A press before the
  start marker or after the end is left out.
- `label`: what to show. A typed string is captioned once, as it starts.
- A clip with no key presses gets `{ "keys": [] }`.

## Demo video footage (`--video`)

`pnpm marketing:clips --video` (plus `--only <beat>`) records the 90-second demo video's long-form beats instead: dark
only, 1280x800 at 125% zoom, paced like a confident human demo, as ProRes masters in `brand/clip-masters/video/` with no
encode. Beats live in `apps/desktop/test/e2e-playwright/marketing-video-scenes.ts`: `speed` (a 50,000-file folder, fast
scrolling, an instant search), `trust`, `rename`, `reach` (a zip and a Git repo's history as folders), `ai`, `hero`, and
`care`. Each take names its key moments (`take.note`), and `video-beats.json` beside the masters lists every beat's
file, size, usable span, key frame, and notes in seconds from the master's start
(`apps/desktop/scripts/marketing-video.ts`).

`pnpm marketing:clips --video2` records the second video's beats ("safe file operations, to and from anywhere") the same
way, into `brand/clip-masters/video2/`: `smb`, `s3`, `phone`, `archives`, `transfers`, `undo`, `hero2`
(`marketing-video2-scenes.ts`). ❗ It needs the Docker fixtures up first
(`apps/desktop/test/smb-servers/start.sh minimal`, `apps/desktop/test/s3-servers/start.sh minimal`). Before the launch
it refills them to look like a family's (`marketing-video2-fixtures.ts`: the SMB share's seed files are replaced, a
`family-photos` bucket, a restic `Backups` repo), clears the clips instance's operation log and saved bucket, and
launches with a virtual MTP phone named `Pixel 8` (`CMDR_VIRTUAL_MTP`, `CMDR_VIRTUAL_MTP_MODEL`). The `s3` beat uploads
into Cmdr's real R2 test bucket instead when the `secret` CLI supplies the account, since a priced provider is what puts
a cost estimate in the copy dialog: the orchestrator writes the secret into the clips instance's file secret store
before launch (it never crosses Playwright or MCP), and after the run deletes the upload and checks that a listing of
the demo prefix comes back empty (`seedPricedS3` and `cleanUpPricedS3` in `marketing-video2-fixtures.ts`).

## What it produces

Into `apps/website/public/clips/` (or `--out`), per clip and theme:

- `{slug}-{theme}-av1.mp4`: SVT-AV1, screen-content mode (`scm=1`), 30 fps, at most 1,500 px wide.
- `{slug}-{theme}-h264.mp4`: the fallback for browsers without AV1 decode; `faststart`, `yuv420p`.
- `{slug}-{theme}.webp`: the poster, cut at the moment the scene marks as representative.
- `{slug}-{theme}.keys.json`: the take's key captions (§ "Key captions").
- `clips.json`: one line per clip with `width`, `height`, `duration`, and `themes`. An `--only` run merges into it.

The lossless masters (ProRes 4444, ~60 MB a second at 2x) and their trim markers stay in `brand/clip-masters/`,
gitignored. Tune an encode setting and re-run with `--encode-only`; no re-recording needed.

## How it works

1. **Launch**: `apps/desktop/scripts/marketing-launch.ts`, shared with `pnpm marketing:shots`: the Playwright-enabled
   E2E binary on its own persistent data dir (`~/Library/Application Support/com.veszelovszki.cmdr-clips`), with
   `CMDR_E2E_MODE` and `CMDR_E2E_START_PATH` unset for the same reasons as the stills.
2. **Stage**: the orchestrator stages a demo tree at `/Users/Shared/Studio` (`CMDR_CLIPS_DEMO_ROOT` overrides) BEFORE
   the launch, and keeps it between runs (`pnpm marketing:clips --remove-demo` deletes it): a small studio's shared
   drive with realistic, non-personal names. A tree staged under the running app reads `<dir>` and `≥` sizes for
   minutes, so `ensureDemoTree` in `marketing-clips-demo.ts` keeps a current tree and only empties its mutable folders.
   ❗ It deletes that folder only when it carries the `.cmdr-clips-demo` marker it wrote, so a real folder at that path
   makes the run refuse instead. `language-switch` records last: after it, the app's saved language can drift back to
   the last previewed one until the next launch (`marketing-clips-slugs.ts`).
3. **Record**: per take, `marketing-clips-take.ts` starts the recorder on the window's CGWindowID and resolves on its
   first frame; the scene then marks start, poster, and end on the wall clock while it drives the real UI.
   `marketing-clips-recorder.swift` is compiled on demand into `~/Library/Caches/com.veszelovszki.cmdr-clips/recorder/`,
   keyed by its source hash. It records the display with every other window excluded, cropped to the window's frame
   (`ops-queue` keeps the queue window too). ❗ Never a window filter (`desktopIndependentWindow`,
   `display(including:)`): macOS 27 then replaces the window's traffic lights with its purple "being shared" pill. The
   recorder also parks the mouse pointer off the window, since a resting pointer hovers buttons and opens tooltips.
4. **Encode**: `apps/desktop/scripts/marketing-clips-encode.ts` trims each master to its markers and encodes. The
   recorder ends the master at its STOP time, not its last frame (ScreenCaptureKit sends frames only on change), so a
   still ending isn't cut short.

Pacing comes from the E2E throttles, set over IPC per scene: `set_test_throttle` paces the copies per file, and
`set_test_rollback_throttle` paces a Rollback-cancelled copy's own reversal, which is what makes the bar visibly drain
(`docs/testing.md` § "E2E env-var hooks").

## When it refuses

- **A `✗` in the preflight**: the line says what's missing. Screen Recording and Automation (System Events) are granted
  to the terminal app, not to Node; restart the terminal after granting.
- **"exists and isn't a clips demo tree"**: a folder without the marker sits at the demo path. Move it, or set
  `CMDR_CLIPS_DEMO_ROOT`.
- **"window N isn't shareable"** or **"No main window of WxH points"**: the window is minimized, on another Space, or
  was resized mid-take.
- **"its take failed this run (an older master is on disk)"**: the encode refuses a master older than the run, so a
  failed take never ships yesterday's clip by accident. `--encode-only` uses whatever is there.
