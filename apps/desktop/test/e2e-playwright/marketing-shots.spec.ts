/**
 * The marketing capture: the brand masters in `brand/screenshots/`, plus the pane
 * rectangles the website hero is cut from.
 *
 * Driven by `pnpm marketing:shots`, never by a bare Playwright run: the orchestrator is
 * what launches a prod-looking app on the persistent shots data dir, clones a warm
 * index into it, and hands this spec the app's pid and data dir. It is a screenshot
 * driver, not a pass/fail suite, so it has its own shard and never joins `all` / `mtp` /
 * `non-mtp`.
 *
 * ❗ It runs with NO fixture tree, photographing real folders, and therefore on
 * `captureTest` (no leak guard). Read
 * `apps/desktop/test/e2e-playwright/DETAILS.md` § "Key decisions" before changing how it is wired.
 */

import { waitBudget } from './wait-budget.js'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { ensureMcpClient, mcpCall } from '../e2e-shared/mcp-client.js'
import { captureTest as test, expect, stopVideoRecording } from './fixtures.js'
import { dispatchMenuCommand, openSettingsWindowViaProd } from './helpers.js'
import { SEARCH_OVERLAY } from './search-helpers.js'
import { insetRect } from './marketing-shots-frame.js'
import type { Rect } from './marketing-shots-frame.js'
import {
  appPid,
  outputDir,
  setWindowSize,
  shootWindow,
  shotsDataDir,
  windowMetrics,
} from './marketing-shots-helpers.js'
import { focusApp } from './marketing-shots-native.js'
import { countDownOnceIfAsked } from './capture-countdown-overlay.js'
import { seedChatThread } from './marketing-shots-thread.js'
import {
  pinVolatileChrome,
  resetTabs,
  setSetting,
  setTheme,
  setViewMode,
  stageCosmetics,
  waitForIndexedSizes,
} from './marketing-stage.js'

/**
 * The main window's logical size, and the one number the website hero depends on: it
 * makes the master 2284x1410 device px, which is the canvas the hero layers are cut
 * from.
 */
const MAIN_WINDOW = { width: 1142, height: 705 }

/**
 * Device pixels trimmed off each pane rectangle, so the window border and the pane
 * divider stay in the hero's FRAME layer. Without it they ride along with a pane as it
 * animates and tear a transparent line down the illustration.
 */
const CUTOUT_INSET = 2

/** What the search master searches for. A word that hits plenty of real files in the repo. */
const SEARCH_QUERY = 'watcher'

/** The repository the panes browse. Aesthetic, but a real tree with real sizes. */
const REPO = process.env.CMDR_SHOTS_BROWSE_ROOT ?? join(process.env.HOME ?? '', 'projects-git', 'vdavid', 'cmdr')

/**
 * The staged layout. The defaults mirror the Total Commander shot on the blog's comparison
 * slider (`src-tauri` left with the cursor on `examples`, `src/lib` right), so the two halves
 * line up, and 120% text reads well in every downscaled surface. Overrides are for one-off
 * `--out` runs only (`docs/guides/screenshots.md` § "One-off layouts"): the website hero's
 * cutouts are measured from whatever this stages. Paths are relative to `REPO`; an empty
 * cursor name leaves that pane's cursor where it lands.
 */
const LEFT_PANE_PATH = join(REPO, process.env.CMDR_SHOTS_LEFT_PATH ?? 'apps/desktop/src-tauri')
const RIGHT_PANE_PATH = join(REPO, process.env.CMDR_SHOTS_RIGHT_PATH ?? 'apps/desktop/src/lib')
const LEFT_CURSOR = process.env.CMDR_SHOTS_LEFT_CURSOR ?? 'examples'
const RIGHT_CURSOR = process.env.CMDR_SHOTS_RIGHT_CURSOR ?? ''
/** `appearance.textSize`, in percent. */
const TEXT_SIZE = Number(process.env.CMDR_SHOTS_TEXT_SIZE ?? '120')

// Staging a master takes several UI round-trips plus up to three shots, and the config's
// 15 s default would cut the first one off. Generous on purpose: on timeout Playwright
// destroys the plugin socket, so every later shot fails with `Not connected`, which
// reads like a crash and buries the real message.
test.setTimeout(waitBudget(1_200_000))

test.describe('marketing masters', () => {
  test.skip(process.platform !== 'darwin', 'The masters are macOS window shots: traffic lights and system corners.')

  test.beforeEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    // `captureTest` carries no auto fixture, so the recorder upstream starts for every
    // test is still running here. A camera driver is the last place that belongs: it
    // burns CPU and CoreGraphics work alongside the very shots it would degrade.
    await stopVideoRecording(page)
    await page.waitForSelector('.file-pane', 15000)
    await ensureMcpClient(page)
    // ❗ The index wait FIRST, then the countdown: the reconcile can take five minutes,
    // and a countdown before it tells David "hands off" five minutes too early (it did).
    // This is the only index gate; nothing earlier can tell "settled" as reliably.
    await waitForIndexedSizes(page, '[marketing-shots]')
    // Before the first test drives anything: front Cmdr, then the hands-off countdown.
    await countDownOnceIfAsked(page, () => {
      focusApp(appPid())
    })
  })

  test('main window, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const metrics = await stageMainWindow(page)

    await setTheme(page, 'dark')
    await shootWindow(page, 'main', 'app-main-dark.webp', metrics, 'dark')

    // Measure the hero cutouts from the SAME staged window that was just photographed.
    // Measuring separately is exactly how the committed rectangles drifted a redesign
    // behind the shot they were supposed to describe.
    const [left, right] = await measurePaneCutouts(page, metrics.scale)

    await setTheme(page, 'light')
    await shootWindow(page, 'main', 'app-main-light.webp', metrics, 'light')

    writeFileSync(
      join(outputDir(), 'hero-cutouts.json'),
      `${JSON.stringify(
        {
          measuredWith: 'apps/desktop/test/e2e-playwright/marketing-shots.spec.ts, on the live DOM',
          window: metrics.device,
          panes: { left, right },
        },
        null,
        2,
      )}\n`,
    )
  })

  test('search, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const metrics = await stageMainWindow(page)

    // `open_search_dialog` runs the query itself, but acks once the dialog has MOUNTED,
    // which is well before it has results.
    await mcpCall('open_search_dialog', { query: SEARCH_QUERY })
    // So gate on the CONTENT, not the container: a dialog holding both a spinner and
    // its results is present long before there is anything to photograph.
    await page.waitForSelector(`${SEARCH_OVERLAY} .result-row`, 30000)

    for (const mode of ['dark', 'light'] as const) {
      await setTheme(page, mode)
      await shootWindow(page, 'main', `search-${mode}.webp`, metrics, mode)
    }

    await dismissSearch()
  })

  test('Ask Cmdr, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await stageMainWindow(page)

    // The store, then the thread, then the rail: `waitForAgentStore` explains the order.
    await waitForAgentStore(page)
    seedChatThread(shotsDataDir())
    await openRail(page)
    // The seeded thread, not a live answer: `marketing-shots-thread.ts` explains why.
    await page.waitForSelector('.ask-cmdr-rail .msg', 15000)

    // ❗ Read the rect the rail actually produced; don't predict it. `growRectForRail`
    // caps at the monitor width, so on a smaller display the panes shrink instead of
    // the window growing, and a hardcoded canvas would fail a perfectly good shot.
    const metrics = await windowMetrics(page, 'main')

    for (const mode of ['dark', 'light'] as const) {
      await setTheme(page, mode)
      await shootWindow(page, 'main', `chat-${mode}.webp`, metrics, mode)
    }

    await closeRail(page)
  })

  test('settings, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await stageMainWindow(page)

    // Through the production multi-window flow, never by routing the main window to
    // `/settings`: that skips the restricted capability ACL a real settings window runs
    // under, and a shot of a window that couldn't exist is worse than no shot.
    const settings = await openSettingsWindowViaProd(page)
    await settings.waitForSelector('.settings-window', 15000)

    // ❗ Read it from the MAIN page. The settings window runs under a restricted
    // capability that doesn't grant `plugin:window|inner_size`, so asking the scoped
    // page fails with an ACL error — which is production behaving correctly, not a bug.
    // Its size tracks the system text scale, so it is read, never assumed.
    const metrics = await windowMetrics(page, 'settings')

    for (const mode of ['dark', 'light'] as const) {
      await setTheme(page, mode)
      await shootWindow(settings, 'settings', `settings-${mode}.webp`, metrics, mode)
    }

    await page.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'settings' })`)
  })
})

/**
 * Puts the main window into the state every master is shot from, and returns its live
 * metrics.
 *
 * Idempotent, and re-run per test on purpose: a previous test leaves the rail open or a
 * dialog up, and a master staged on top of that is the failure this pipeline exists to
 * stop being invisible.
 */
async function stageMainWindow(page: TauriPage): Promise<Awaited<ReturnType<typeof windowMetrics>>> {
  await closeRail(page)
  // ❗ Resize with the rail CLOSED. With it open each pane measures ~430 px instead of
  // ~570 px, and the hero cutouts would be measured from a window nobody ships.
  await setWindowSize(page, 'main', MAIN_WINDOW.width, MAIN_WINDOW.height)
  await expect
    .poll(async () => (await windowMetrics(page, 'main')).logical.width, { timeout: waitBudget(5000) })
    .toBe(MAIN_WINDOW.width)

  await waitForIndexedSizes(page, '[marketing-shots]')
  await stageCosmetics(page)
  // Set on every run, the default included: the data dir would otherwise carry a one-off
  // run's zoom into the next brand masters.
  await setSetting(page, 'appearance.textSize', TEXT_SIZE)
  // Dotfiles on, like the Total Commander shot the main master sits beside on the blog.
  await setSetting(page, 'listing.showHiddenFiles', true)
  await stagePanes(page)
  await pinVolatileChrome(page)

  const metrics = await windowMetrics(page, 'main')
  // Every margin this pipeline gates on is a device-pixel number, so a 1x display has
  // to fail saying that rather than failing at arithmetic that looks broken.
  expect(metrics.scale, 'the masters are retina shots; run this on a 2x display').toBe(2)
  return metrics
}

async function stagePanes(page: TauriPage): Promise<void> {
  await resetTabs('left')
  await resetTabs('right')

  // Full on the left, brief on the right: the asymmetry is the point, it shows both view
  // modes in one frame. Set explicitly on BOTH panes rather than left to the data dir,
  // which remembers whatever the last run (or a hand-driven session) left behind.
  await setViewMode('left', 'full')
  await setViewMode('right', 'brief')

  await mcpCall('nav_to_path', { pane: 'left', path: LEFT_PANE_PATH })
  await mcpCall('nav_to_path', { pane: 'right', path: LEFT_PANE_PATH })
  // The pinned tab's lock glyph is the visual interest in the right pane, and pinning
  // BEFORE opening the second tab is what leaves it behind the active one.
  await mcpCall('tab', { pane: 'right', action: 'set_pinned', pinned: true })
  await mcpCall('tab', { pane: 'right', action: 'new' })
  await mcpCall('nav_to_path', { pane: 'right', path: RIGHT_PANE_PATH })

  if (LEFT_CURSOR) {
    await mcpCall('move_cursor', { pane: 'left', filename: LEFT_CURSOR }).catch(() => {
      // The cursor is aesthetic; a renamed directory shouldn't fail a whole round.
    })
  }
  if (RIGHT_CURSOR) await mcpCall('move_cursor', { pane: 'right', filename: RIGHT_CURSOR }).catch(() => {})

  // The left pane focused: its cursor row is the one that reads as "you are here".
  const leftFocused = await page.evaluate<boolean>(
    `document.querySelectorAll('.file-pane')[0]?.classList.contains('is-focused') ?? false`,
  )
  if (!leftFocused) await mcpCall('switch_pane', {})
}

/**
 * Waits until the agent store is open on this launch, which `seedChatThread` needs next: it
 * writes into `main.db`, and only an open store has run this launch's migrations.
 *
 * The one-time Ask Cmdr opt-in probe is the cheapest honest read: it answers
 * `storeUnavailable` exactly when the store can't be read. The rail itself opens past its
 * gate because `scripts/marketing-shots.ts` (through `prepareCaptureInstance`) turns `askCmdr.enabled` on,
 * and proactive wakes stay off for this instance, so nothing starts threads mid-run.
 */
async function waitForAgentStore(page: TauriPage): Promise<void> {
  await expect
    .poll(() => page.evaluate<string>(`window.__TAURI_INTERNALS__.invoke('ask_cmdr_legacy_opt_in')`), {
      message: 'the agent store never opened, so the seeded thread has nowhere to go',
      timeout: waitBudget(15000),
    })
    .not.toBe('storeUnavailable')
}

async function railOpen(page: TauriPage): Promise<boolean> {
  return page.evaluate<boolean>(`document.querySelector('.ask-cmdr-rail') !== null`)
}

/** Toggles the rail open, re-dispatching inside the poll past the 300 ms double-fire guard. */
async function openRail(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (await railOpen(page)) return true
        await dispatchMenuCommand(page, 'askCmdr.toggle')
        return railOpen(page)
      },
      { timeout: waitBudget(10000) },
    )
    .toBe(true)
}

async function closeRail(page: TauriPage): Promise<void> {
  await expect
    .poll(
      async () => {
        if (!(await railOpen(page))) return true
        await dispatchMenuCommand(page, 'askCmdr.toggle')
        return !(await railOpen(page))
      },
      { timeout: waitBudget(10000) },
    )
    .toBe(true)
}

/** Closes the search dialog through its own close path. ❌ Never `keyboard.press('Escape')`. */
async function dismissSearch(): Promise<void> {
  await mcpCall('dialog', { action: 'close', type: 'search' }).catch(() => {
    // Already gone, which is the state we wanted.
  })
}

/**
 * The two pane rectangles, in device pixels relative to the window's top-left.
 *
 * Each pane's list container gives the left edge and width; the row area inside it gives
 * the top, which is below the column headers; the container's bottom ends it, above the
 * status bar. So the hero's holes frame the file lists and nothing else, and they follow
 * the layout instead of a constant that outlives it.
 *
 * ❗ Both view modes, because the masters show one of each: full mode paints
 * `.full-list-container` + `.listbox-region`, brief mode `.brief-list-container` +
 * `.brief-list`. A full-mode-only query throws on the brief pane and takes the hero
 * cutouts down with it.
 */
async function measurePaneCutouts(page: TauriPage, scale: number): Promise<[Rect, Rect]> {
  const measured = await page.evaluate<Rect[]>(
    `(() => {
       const dpr = ${String(scale)}
       return [...document.querySelectorAll('.file-pane')].map((pane) => {
         const container = pane.querySelector('.full-list-container, .brief-list-container')
         const rowArea = pane.querySelector('.listbox-region, .brief-list')
         if (!container || !rowArea) return null
         const box = container.getBoundingClientRect()
         const rows = rowArea.getBoundingClientRect()
         return {
           x: Math.round(box.x * dpr),
           y: Math.round(rows.y * dpr),
           width: Math.round(box.width * dpr),
           height: Math.round((box.bottom - rows.y) * dpr),
         }
       })
     })()`,
  )
  expect(measured, 'the hero needs exactly two panes to cut from').toHaveLength(2)
  // A `null` here means a pane rendered neither list shape, so the hero would silently
  // get a rectangle built from `undefined`. Say which pane, and stop.
  expect(measured.filter(Boolean), 'both panes must show a file list to measure').toHaveLength(2)
  const [left, right] = measured.map((rect) => insetRect(rect, CUTOUT_INSET))
  return [left, right]
}
