# Updating screenshots everywhere

How to reshoot Cmdr's marketing screenshots and refresh every place they appear. One command does the capture; the rest
of this page is what to do with the results, and what to look at when a run refuses.

Not to be confused with the translator screenshots (`pnpm i18n:shots`), which photograph ~150 UI surfaces for
`docs/guides/i18n-translation.md`. Different audience, different pipeline, one shared launch layer
(`apps/desktop/scripts/capture-runtime.ts`).

## Reshoot

```bash
pnpm marketing:shots                     # into brand/screenshots/
pnpm marketing:shots --out ~/Desktop/qa  # somewhere else, leaving brand/ untouched
```

It runs on the Playwright lane's E2E binary, rebuilt first only when the tree changed, so a master can never come from a
stale build.

❗ **The machine must be left alone while it shoots.** macOS composites only the frontmost window and colours the
traffic lights only for the key one, so every shot takes the front position first; clicking into another app mid-run
costs retries. Right before the shoot, Cmdr comes to the front with its window covered in orange: a three-second
countdown and the expected duration, swallowing clicks and typing while it's up. When it clears, hands off. You can keep
working until then (the build and any index wait come first). Unlike `pnpm i18n:shots`, this does not refuse to start
behind another app, because it claims the front through System Events and then confirms, after every shutter, that Cmdr
still held it. Overlay and estimate: `apps/desktop/scripts/DETAILS.md` § "The pre-capture countdown".

A quiet run takes about 25 seconds and writes nine files (the overlay's figure calibrates itself from real runs). The
one exception is the drive index: see § "When it waits".

## What it produces

Into `brand/screenshots/` (or `--out`):

- `app-main-{dark,light}.webp` — the two-pane main view. Feeds the README, the website hero, and the directory listings,
  so all three can never drift apart.
- `search-{dark,light}.webp`, `chat-{dark,light}.webp`, `settings-{dark,light}.webp` — listings only. The Ask Cmdr rail
  widens the window, so that pair lands on a wider canvas than the rest.
- `hero-cutouts.json` — the two pane rectangles, measured off the live DOM in the same test that took the shot, which is
  what keeps the hero geometry from drifting a redesign behind its own screenshot.

Every master is a real macOS window shot WITHOUT its shadow (`screencapture -o`): the window with the system's own
rounded corners on transparency, plus the 1 px outline macOS draws around a window, which `-o` drops and the run traces
back on (`windowOutlineArgs` in `marketing-shots-frame.ts`). `app-main` is 2286x1412: a 2284x1410 window at `+1+1`. No
shadow is baked in on purpose: it was invisible on dark pages and only shrank the window in the README and the listings,
so each surface that wants one draws it in CSS (the hero, the blog's lightbox).

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
- **Blog's Total Commander comparison**: crop each main-window master to the bare window at 1,400 px wide, the slider's
  size:
  `magick brand/screenshots/app-main-$m.webp -crop 2284x1410+1+1 +repage -resize 1400x -quality 85 apps/website/public/blog/total-commander-for-macos/cmdr-$m.webp`
  for `m` in `dark` and `light`.
- **App directories**: re-upload on each listing, and update the matching file in `brand/listings/` in the same pass.

## What the masters show

The main window mirrors the Total Commander shot it sits beside on the blog: `apps/desktop/src-tauri` on the left with
the cursor on `examples`, `apps/desktop/src/lib` on the right (behind a pinned `src-tauri` tab), hidden files shown, and
text at 120% so it stays readable in every downscaled surface. All of it is staged in `marketing-shots.spec.ts` on every
run, because the shots data dir is persistent.

## One-off layouts

For a shot that has to match something else, override the staged layout with env vars and ❗ always `--out` somewhere
outside `brand/`, so the masters (and the hero cut from them) keep the standard layout.

```bash
CMDR_SHOTS_LEFT_PATH=apps/desktop/src/lib CMDR_SHOTS_LEFT_CURSOR=file-explorer \
CMDR_SHOTS_RIGHT_PATH=apps/desktop/src-tauri/src CMDR_SHOTS_RIGHT_CURSOR=file_system \
CMDR_SHOTS_TEXT_SIZE=100 pnpm marketing:shots --out /tmp/one-off
```

- `CMDR_SHOTS_LEFT_PATH` / `CMDR_SHOTS_RIGHT_PATH`: folders relative to the browsed repo. The right pane's pinned tab
  behind the active one opens the left path.
- `CMDR_SHOTS_LEFT_CURSOR` / `CMDR_SHOTS_RIGHT_CURSOR`: the row to put the cursor on; empty leaves it where it lands.
- `CMDR_SHOTS_TEXT_SIZE`: `appearance.textSize` in percent (default 120). Every run sets it, so a one-off zoom never
  leaks into the next masters through the persistent shots data dir.

## How it works, in one paragraph

`apps/desktop/scripts/marketing-shots.ts` launches the Playwright-enabled binary on a persistent data dir of its own
(`~/Library/Application Support/com.veszelovszki.cmdr-shots`, with its drive index in
`~/Library/Caches/com.veszelovszki.cmdr-shots/drive-index`), then runs `marketing-shots.spec.ts` on its own shard. The
spec stages each shot through the real UI and photographs it with `screencapture -l -o`, verifying the bytes before it
keeps them. Design and rationale (why no baked-in shadow, and why no dedicated `shots` app mode):
`apps/desktop/test/e2e-playwright/DETAILS.md` § "Key decisions".

Two deliberate choices worth knowing before you change anything:

- **`CMDR_E2E_MODE` stays unset.** It paints the blue `E2E MODE` title bar and sets `ActivationPolicy::Prohibited`,
  which makes the window permanently unable to become key, so its traffic lights photograph grey. So an E2E-mode run
  could not produce a master even in principle.
- **`CMDR_E2E_START_PATH` stays unset, and the shard skips the fixture machinery.** That variable arms the suite's
  post-test guard, which deletes anything not in the fixture manifest — and this is the one run that photographs real
  folders. ❌ Never set it for this shard.

## When it refuses

The failures are named on purpose; read the message before changing anything.

- **"the window was not focused when it was shot"** — System Events named another app frontmost right after the shutter.
  Something took the front position mid-run. Leave the machine alone and re-run.
- **"…so the shadow came along. The shutter must pass `-o`"** — the capture is bigger than the window; someone dropped
  `-o` from `captureWindow`.
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
instance before launching. It uses SQLite's backup API in one step (`node:sqlite`), not `cp`: the index is in WAL mode,
so copying it out from under a running app is a torn read. 1.1 GB takes about two seconds. Why one step, and not the
`sqlite3` CLI's `.backup` (which livelocks against a running Cmdr) or `VACUUM INTO` (which needs Cmdr's collation):
`cloneProdIndex` in `apps/desktop/scripts/marketing-shots.ts`.

❗ The copy skips the wait only when production is current. Startup rebuilds when the index's stored FSEvents id is more
than ten million behind the system's current one, and on a machine that compiles all day that's about an hour (14
million in 90 minutes with two E2E builds). A RUNNING production Cmdr keeps its index seconds behind, so the run copies
whichever index is further along by that same number, and with production open the launch replays the journal instead of
rebuilding. With production quit for hours, both are behind and the instance reconciles, exactly as production itself
would on restart: about five minutes, once, and the run says so while it waits. That wait comes BEFORE the orange
countdown, so the machine is yours until it ends.

❌ Don't answer this by writing the current event id into the copied index. That claims it has seen changes it hasn't,
and trades one five-minute wait for permanently stale sizes.

No production install or no readable index both fall through to a normal scan, with a line saying so.

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

The chat master runs off a seeded conversation (`apps/desktop/test/e2e-playwright/marketing-shots-thread.ts`), so it
needs no provider, no API key, and no spend, and says the same thing every run. Its copy is a draft for David's review
like any other user-facing string, and it must only describe things Cmdr actually does.

The spec accepts Ask Cmdr's consent through the app's own command, then seeds the thread, right before it opens the
rail. So the seed never hardcodes a consent version, and it never writes to a schema this launch hasn't migrated yet.
The orchestrator keeps `askCmdr.proactive` off on every run, because an accepted consent plus the E2E fake provider
would otherwise let a wake start threads of its own mid-run.
