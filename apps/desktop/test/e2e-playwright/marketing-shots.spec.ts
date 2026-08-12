/**
 * The marketing capture: the brand masters in `brand/screenshots/`, plus the pane
 * rectangles the website hero is cut from.
 *
 * Driven by `pnpm marketing:shots`, never by a bare Playwright run: the orchestrator is
 * what launches a prod-looking app on the persistent shots data dir, clones a warm
 * index into it, seeds the chat thread, and hands this spec the app's pid. It is a
 * screenshot driver, not a pass/fail suite, so it has its own shard and never joins
 * `all` / `mtp` / `non-mtp`.
 *
 * ❗ It runs with NO fixture tree, photographing real folders, and therefore on
 * `captureTest` (no leak guard). Read
 * `docs/specs/marketing-screenshot-pipeline-plan.md` before changing how it is wired.
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { ensureMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'
import { captureTest as test, expect } from './fixtures.js'
import { dispatchMenuCommand, openSettingsWindowViaProd } from './helpers.js'
import { SEARCH_OVERLAY } from './search-helpers.js'
import { insetRect } from './marketing-shots-frame.js'
import { indexIsSettled, parsePaneTabs } from './marketing-shots-state.js'
import type { Rect } from './marketing-shots-frame.js'
import { outputDir, setWindowSize, shootWithShadow, windowMetrics } from './marketing-shots-helpers.js'

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
const LEFT_PANE_PATH = join(REPO, 'apps', 'desktop', 'src', 'lib')
const RIGHT_PANE_PATH = join(REPO, 'apps', 'desktop', 'src-tauri', 'src')

// Staging a master takes several UI round-trips plus up to three shots, and the config's
// 15 s default would cut the first one off. Generous on purpose: on timeout Playwright
// destroys the plugin socket, so every later shot fails with `Not connected`, which
// reads like a crash and buries the real message.
test.setTimeout(1_200_000)

test.describe('marketing masters', () => {
  test.skip(process.platform !== 'darwin', 'The masters are macOS window shots: traffic lights and a system shadow.')

  test.beforeEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await page.waitForSelector('.file-pane', 15000)
    await ensureMcpClient(page)
  })

  test('main window, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    const metrics = await stageMainWindow(page)

    await setTheme(page, 'dark')
    await shootWithShadow(page, 'main', 'app-main-dark.png', metrics)

    // Measure the hero cutouts from the SAME staged window that was just photographed.
    // Measuring separately is exactly how the committed rectangles drifted a redesign
    // behind the shot they were supposed to describe.
    const [left, right] = await measurePaneCutouts(page, metrics.scale)

    await setTheme(page, 'light')
    await shootWithShadow(page, 'main', 'app-main-light.png', metrics)

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
      await shootWithShadow(page, 'main', `search-${mode}.png`, metrics)
    }

    await dismissSearch()
  })

  test('Ask Cmdr, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await stageMainWindow(page)

    await openRail(page)
    // The seeded thread, not a live answer: `marketing-shots-thread.ts` explains why.
    await page.waitForSelector('.ask-cmdr-rail .msg', 15000)

    // ❗ Read the rect the rail actually produced; don't predict it. `growRectForRail`
    // caps at the monitor width, so on a smaller display the panes shrink instead of
    // the window growing, and a hardcoded canvas would fail a perfectly good shot.
    const metrics = await windowMetrics(page, 'main')

    for (const mode of ['dark', 'light'] as const) {
      await setTheme(page, mode)
      await shootWithShadow(page, 'main', `chat-${mode}.png`, metrics)
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
      await shootWithShadow(settings, 'settings', `settings-${mode}.png`, metrics)
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
    .poll(async () => (await windowMetrics(page, 'main')).logical.width, { timeout: 5000 })
    .toBe(MAIN_WINDOW.width)

  await waitForIndexedSizes(page)
  await stagePanes(page)

  const metrics = await windowMetrics(page, 'main')
  // Every margin this pipeline gates on is a device-pixel number, so a 1x display has
  // to fail saying that rather than failing at arithmetic that looks broken.
  expect(metrics.scale, 'the masters are retina shots; run this on a 2x display').toBe(2)
  return metrics
}

/**
 * Waits until folder sizes are real numbers rather than hourglasses.
 *
 * ❗ Not cosmetic. While the drive index reconciles, every size cell shows an hourglass
 * and every folder size reads `≥`, which is what a whole round of unusable masters looks
 * like — it has happened. The orchestrator clones a warm index in to make this instant;
 * this is the gate that proves it worked.
 */
async function waitForIndexedSizes(page: TauriPage): Promise<void> {
  await ensureMcpClient(page)
  let announced = false
  await expect
    .poll(
      async () => {
        const settled = indexIsSettled(await mcpReadResource('cmdr://state?include=volumes'))
        if (!settled && !announced) {
          announced = true
          console.log(
            '[marketing-shots] the drive index is still catching up. This happens when the copied index is ' +
              'more than ten million FSEvents behind, which on a busy machine means hours, not days. ' +
              'It takes about five minutes, once, and later runs reuse the result.',
          )
        }
        return settled
      },
      {
        // Long enough for a full reconcile of a ~6 M entry drive (measured 284 s for the
        // scan plus aggregation), with room for a loaded machine. ❌ Don't trim this to
        // "make the suite faster": the alternative to waiting is shipping hourglasses.
        timeout: 900_000,
        intervals: [1000],
        message: 'the drive index never settled, so every folder size would photograph as an hourglass',
      },
    )
    .toBe(true)
}

/**
 * The two-pane arrangement the masters show: a source tree on the left, the Rust
 * backend on the right behind a pinned tab.
 *
 * ❗ Unpin before closing. `close` and `close_others` deliberately SKIP pinned tabs, so
 * a "clean up" that forgets this leaves the pane with three tabs and the shot shows a
 * layout nobody asked for.
 */
async function resetTabs(pane: 'left' | 'right'): Promise<void> {
  // ❗ Unpin FIRST. `close_others` deliberately skips pinned tabs, so a data dir that
  // remembers yesterday's pinned tab ends up with three tabs in the pane and a shot of
  // a layout nobody asked for. This is why the reset reads the live tab list rather
  // than firing `close_others` and hoping.
  const state = await mcpReadResource('cmdr://state?include=panes')
  for (const tab of parsePaneTabs(state, pane)) {
    if (tab.pinned) await mcpCall('tab', { pane, action: 'set_pinned', tabId: tab.id, pinned: false })
  }
  await mcpCall('tab', { pane, action: 'close_others' }).catch(() => {
    // A single-tab pane has nothing to close, and says so rather than succeeding.
  })
}

async function stagePanes(page: TauriPage): Promise<void> {
  await resetTabs('left')
  await resetTabs('right')

  await mcpCall('nav_to_path', { pane: 'left', path: LEFT_PANE_PATH })
  await mcpCall('nav_to_path', { pane: 'right', path: LEFT_PANE_PATH })
  // The pinned tab's lock glyph is the visual interest in the right pane, and pinning
  // BEFORE opening the second tab is what leaves it behind the active one.
  await mcpCall('tab', { pane: 'right', action: 'set_pinned', pinned: true })
  await mcpCall('tab', { pane: 'right', action: 'new' })
  await mcpCall('nav_to_path', { pane: 'right', path: RIGHT_PANE_PATH })

  await mcpCall('move_cursor', { pane: 'left', filename: 'file-explorer' }).catch(() => {
    // The cursor is aesthetic; a renamed directory shouldn't fail a whole round.
  })
  await mcpCall('move_cursor', { pane: 'right', filename: 'file_system' }).catch(() => {})

  // The left pane focused: its cursor row is the one that reads as "you are here".
  const leftFocused = await page.evaluate<boolean>(
    `document.querySelectorAll('.file-pane')[0]?.classList.contains('is-focused') ?? false`,
  )
  if (!leftFocused) await mcpCall('switch_pane', {})
}

/**
 * Changes a setting the way the app's own MCP server does: emit `mcp-set-setting` and
 * let the main window's bridge apply it (`settings/mcp-main-bridge.ts`).
 *
 * ❗ Not `invoke('set_setting')` — there is no such Tauri command; the MCP tool is a
 * round-trip THROUGH the frontend. And not a direct CSS or `@tauri-apps/api/app` poke
 * either: those change how the app looks without changing what it thinks, so the
 * settings master would photograph a radio button disagreeing with the window around it.
 */
async function setSetting(page: TauriPage, settingId: string, value: unknown): Promise<void> {
  await page.evaluate(
    `window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
       event: 'mcp-set-setting',
       payload: { requestId: 'marketing-shots', settingId: ${JSON.stringify(settingId)}, value: ${JSON.stringify(value)} }
     })`,
  )
}

/** Switches the app between dark and light through the real setting, not a CSS override. */
async function setTheme(page: TauriPage, mode: 'dark' | 'light'): Promise<void> {
  await setSetting(page, 'theme.mode', mode)
  // The theme reaches the UI through Tauri's per-app theme API, so the honest readback
  // is the media query the stylesheet itself keys off, not a class we could set.
  await expect
    .poll(async () => page.evaluate<boolean>(`window.matchMedia('(prefers-color-scheme: dark)').matches`), {
      timeout: 10000,
    })
    .toBe(mode === 'dark')
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
      { timeout: 10000 },
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
      { timeout: 10000 },
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
 * Each pane's `.full-list-container` gives the left edge and width; its
 * `.listbox-region` gives the top, which is below the column headers; the container's
 * bottom ends it, above the status bar. So the hero's holes frame the file lists and
 * nothing else, and they follow the layout instead of a constant that outlives it.
 */
async function measurePaneCutouts(page: TauriPage, scale: number): Promise<[Rect, Rect]> {
  const measured = await page.evaluate<Rect[]>(
    `(() => {
       const dpr = ${String(scale)}
       return [...document.querySelectorAll('.file-pane')].map((pane) => {
         const box = pane.querySelector('.full-list-container').getBoundingClientRect()
         const rows = pane.querySelector('.listbox-region').getBoundingClientRect()
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
  const [left, right] = measured.map((rect) => insetRect(rect, CUTOUT_INSET))
  return [left, right]
}
