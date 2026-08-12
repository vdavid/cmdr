# Updating screenshots everywhere

How to reshoot Cmdr's marketing screenshots and refresh every place they appear. One command does the capture; the rest
of this page is what to do with the results, and what to look at when a run refuses.

Not to be confused with the translator screenshots (`pnpm i18n:capture`), which photograph ~150 UI surfaces for
`docs/guides/i18n-translation.md`. Different audience, different pipeline, one shared launch layer
(`apps/desktop/scripts/capture-runtime.ts`).

## Reshoot

```bash
pnpm marketing:shots                     # into brand/screenshots/
pnpm marketing:shots --build             # rebuild the Playwright binary first
pnpm marketing:shots --out ~/Desktop/qa  # somewhere else, leaving brand/ untouched
```

❗ **Leave the machine alone while it runs.** macOS draws the wide window shadow only for the KEY window, so every shot
takes the front position first; clicking into another app mid-run costs retries. Unlike `pnpm i18n:capture`, this does
not refuse to start behind another app, because it claims the front through System Events and then proves it in the
pixels.

A quiet run takes about 25 seconds and writes nine files. The one exception is the drive index: see § "When it waits".

## What it produces

Into `brand/screenshots/` (or `--out`):

- `app-main-{dark,light}.webp` — the two-pane main view. Feeds the README, the website hero, and the directory listings,
  so all three can never drift apart.
- `search-{dark,light}.webp`, `chat-{dark,light}.webp`, `settings-{dark,light}.webp` — listings only. The Ask Cmdr rail
  widens the window, so that pair lands on a wider canvas than the rest.
- `hero-cutouts.json` — the two pane rectangles, measured off the live DOM in the same test that took the shot, which is
  what keeps the hero geometry from drifting a redesign behind its own screenshot.

Every master is a real macOS window shot: the window plus its focused shadow, on transparency. `app-main` is 2508x1634
around a 2284x1410 window at `+112+76`.

**Lossless WebP**, so a master is pixel-identical to the PNG the shutter takes (`magick compare -metric AE` reads 0) at
about a fifth of the bytes. That is what keeps a reshoot from adding ~8 MB of undeltifiable blobs to git every time,
which is also why ❗ the conversion must never become lossy: these are the originals every other surface is cut from.
The shutter and every pixel gate still work on the PNG; only the file that lands in `brand/` is WebP, which is why the
run needs ImageMagick and refuses to start without it. Need a PNG for an uploader that rejects WebP:
`magick app-main-dark.webp app-main-dark.png`.

## Then refresh each consumer

- **README**: nothing to edit; it points at `brand/screenshots/app-main-{dark,light}.webp`. Commit the new masters.
- **Website hero**: `apps/website/scripts/regenerate-hero.sh`, which reads the masters and `hero-cutouts.json`. Details
  in `apps/website/public/hero/DETAILS.md`.
- **App directories**: re-upload on each listing, and update the matching file in `brand/listings/` in the same pass.

## How it works, in one paragraph

`apps/desktop/scripts/marketing-shots.ts` launches the Playwright-enabled binary on a persistent data dir of its own
(`~/Library/Application Support/com.veszelovszki.cmdr-shots`), then runs `marketing-shots.spec.ts` on its own shard. The
spec stages each shot through the real UI and photographs it with `screencapture -l`, verifying the bytes before it
keeps them. Design and rationale: `docs/specs/marketing-screenshot-pipeline-plan.md`.

Two deliberate choices worth knowing before you change anything:

- **`CMDR_E2E_MODE` stays unset.** It paints the blue `E2E MODE` title bar and sets `ActivationPolicy::Prohibited`,
  which makes the window permanently unable to become key, and only a key window gets the wide shadow. So an E2E-mode
  run could not produce a master even in principle.
- **`CMDR_E2E_START_PATH` stays unset, and the shard skips the fixture machinery.** That variable arms the suite's
  post-test guard, which deletes anything not in the fixture manifest — and this is the one run that photographs real
  folders. ❌ Never set it for this shard.

## When it refuses

The failures are named on purpose; read the message before changing anything.

- **"the window was not focused when it was shot"** — its shadow measured 68/52 instead of 112/76. Something took the
  front position mid-run. Leave the machine alone and re-run.
- **"the capture has nothing opaque in it"** — a blank frame, same cause: macOS stops compositing a window that isn't
  frontmost.
- **"the photographed window is NxM, but the app reports…"** — something resized the window between staging and the
  shutter.
- **"`osascript` failed … not allowed assistive access"** — grant the terminal Accessibility permission. Screen
  Recording is the matching one for `screencapture`.
- **"the drive index never settled"** — see below.

## When it waits

Folder sizes come from the drive index. While it reconciles, every size cell shows an hourglass and folder sizes read
`≥`, which is a whole round of unusable masters, so the spec refuses to shoot until the index has settled.

To make that fast, the orchestrator copies the installed production app's index (`index-root.db`) into the shots
instance before launching. It uses SQLite's online backup, not `cp`: the index is in WAL mode, so copying it out from
under a running app is a torn read. 930 MB takes about two seconds.

❗ The copy does **not** always skip the wait. Startup rebuilds when the index's stored FSEvents id is more than ten
million behind the system's current one, and on a machine that compiles all day that gap is hours rather than days
(measured at 28 million overnight). So a copy from a production app that last scanned yesterday still reconciles,
exactly as production itself would on restart. It costs about five minutes, once, and the run says so while it waits.
Back-to-back runs reuse the settled index and are instant.

❌ Don't answer this by writing the current event id into the copied index. That claims it has seen changes it hasn't,
and trades one five-minute wait for permanently stale sizes.

No production install, no readable index, or no `sqlite3` all fall through to a normal scan, with a line saying so.

## Changing what the shots show

Everything is staged in `apps/desktop/test/e2e-playwright/marketing-shots.spec.ts`: pane paths, the pinned-tab
arrangement, the view mode per pane (full on the left, brief on the right), the search query, which settings section is
open. Anything VISIBLE in a shot is set there, on every run, not in the orchestrator's `seedSettingsIfNew` — that one
writes only on a fresh data dir, and the shots instance is persistent, so a look seeded once can't be changed without
deleting the instance.

Two bits of chrome are pinned rather than staged, because they photograph whatever the machine happens to be doing:

- **The window title** shows `Cmdr`, not the `Cmdr – Personal use only` the unlicensed shots instance computes.
- **The repo chip** shows a clean `main`, not the working copy's real `+14` / dirty state, which changes between runs.

`pinVolatileChrome` rewrites the rendered values and keeps them rewritten through a `MutationObserver` (both are
reactive: the chip repaints on any repo watcher event, the title on any license event). ❗ It paints over the RENDER
only, and must stay that way: no license is written, no app state changes. A master may show a chosen state, never a
fake one the app itself would act on.

The traps already encoded there, so you don't rediscover them:

- The hero cutouts measure BOTH list shapes (`.full-list-container` + `.listbox-region` in full mode,
  `.brief-list-container` + `.brief-list` in brief). A full-mode-only query throws on the brief pane.
- Resize with the Ask Cmdr rail CLOSED. With it open each pane measures ~430 px instead of ~570 px, and the hero cutouts
  would come from a window nobody ships.
- Unpin before closing tabs. `close_others` deliberately skips pinned tabs, so a remembered pin leaves a third tab in
  the shot.
- The settings window's size is read from the MAIN window. Its own restricted capability rejects
  `plugin:window|inner_size`, which is production behaving correctly.

The chat master runs off a seeded conversation (`apps/desktop/scripts/marketing-shots-thread.ts`), so it needs no
provider, no API key, and no spend, and says the same thing every run. Its copy is a draft for David's review like any
other user-facing string, and it must only describe things Cmdr actually does.
