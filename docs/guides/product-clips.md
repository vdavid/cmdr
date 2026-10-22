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
```

- ❗ **Leave the machine alone while it records.** Each take brings Cmdr to the front: ScreenCaptureKit records a
  covered window fine, but WKWebView starves `requestAnimationFrame` in an occluded one, so a clip recorded behind
  another app shows frozen progress bars.
- A full run is roughly 14 takes of ~10–15 s each plus staging, a few minutes in all, unless the drive index has to
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
- `search`: a query typed, results landing.
- `language-switch`: Settings > Appearance > Language, the list open, ArrowDown twelve times; the picker previews each
  language as it's highlighted, so the window re-renders in each.
- `viewer`: a Markdown README, a photo, a text file, then a binary as bytes and hex, one after another in one window.
- `settings`: a pan through the first eight Settings sections.

## What it produces

Into `apps/website/public/clips/` (or `--out`), per clip and theme:

- `{slug}-{theme}-av1.mp4`: SVT-AV1, screen-content mode (`scm=1`), 30 fps, at most 1,500 px wide.
- `{slug}-{theme}-h264.mp4`: the fallback for browsers without AV1 decode; `faststart`, `yuv420p`.
- `{slug}-{theme}.webp`: the poster, cut at the moment the scene marks as representative.
- `clips.json`: one line per clip with `width`, `height`, `duration`, and `themes`. An `--only` run merges into it.

The lossless masters (ProRes 4444, ~60 MB a second at 2x) and their trim markers stay in `brand/clip-masters/`,
gitignored. Tune an encode setting and re-run with `--encode-only`; no re-recording needed.

## How it works

1. **Launch**: `apps/desktop/scripts/marketing-launch.ts`, shared with `pnpm marketing:shots`: the Playwright-enabled
   E2E binary on its own persistent data dir (`~/Library/Application Support/com.veszelovszki.cmdr-clips`), with
   `CMDR_E2E_MODE` and `CMDR_E2E_START_PATH` unset for the same reasons as the stills.
2. **Stage**: `marketing-clips.spec.ts` stages a demo tree at `/Users/Shared/Studio` (`CMDR_CLIPS_DEMO_ROOT` overrides):
   a small studio's shared drive with realistic, non-personal names. ❗ It deletes that folder only when it carries the
   `.cmdr-clips-demo` marker it wrote, so a real folder at that path makes the run refuse instead.
3. **Record**: per take, `marketing-clips-take.ts` starts the recorder on the window's CGWindowID and resolves on its
   first frame; the scene then marks start, poster, and end on the wall clock while it drives the real UI.
   `marketing-clips-recorder.swift` is compiled on demand into `~/Library/Caches/com.veszelovszki.cmdr-clips/recorder/`,
   keyed by its source hash. One window records through `desktopIndependentWindow` without its shadow; `ops-queue` adds
   the queue window through a display filter cropped to the main window's frame.
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
