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
import { indexIsSettled, parsePaneTabs, parsePaneView } from './marketing-shots-state.js'
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

/**
 * The window title the masters carry.
 *
 * The shots instance holds no license, so the app computes `Cmdr – Personal use only`
 * (`licensing/app_status.rs::get_window_title`). The masters show the plain brand name
 * instead, which is a deliberate marketing choice, applied to the rendered title only:
 * the instance stays unlicensed, and nothing here mints or stores a license.
 */
const MASTER_TITLE = 'Cmdr'

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
  await stageCosmetics(page)
  await stagePanes(page)
  await pinVolatileChrome(page)

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
 *
 * Two conditions, because `indexStatus` alone isn't the whole truth: a volume reads
 * `fresh` while a replay or an aggregation pass is still running, and that pass paints
 * the status corner's hourglass into the top-right of every master. So the pixels get a
 * vote — the corner must be empty too (`$lib/indexing/IndexingStatusIndicator.svelte`).
 */
async function waitForIndexedSizes(page: TauriPage): Promise<void> {
  await ensureMcpClient(page)
  let announced = false
  await expect
    .poll(
      async () => {
        const settled =
          indexIsSettled(await mcpReadResource('cmdr://state?include=volumes')) &&
          !(await page.evaluate<boolean>(`document.querySelector('.indexing-status') !== null`))
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

/**
 * The look of the file lists, set every run rather than seeded once.
 *
 * The shots data dir is persistent, so a setting written at creation time can't be
 * changed later without deleting the instance — and these are exactly the ones a master
 * is judged on. The orchestrator's `seedSettingsIfNew` covers suppressions (analytics,
 * update toasts); anything VISIBLE in a shot belongs here, where every run re-applies it.
 */
async function stageCosmetics(page: TauriPage): Promise<void> {
  // Rainbow size tiers: the color is most of what makes a file list read as a product
  // shot rather than a directory dump.
  await setSetting(page, 'appearance.sizeColors', 'rainbow')
}

/**
 * Puts a pane in a view mode, skipping the call when it's already there.
 *
 * ❗ The skip is required, not an optimization. `set_view_mode` acks on the pane's state
 * generation advancing, so setting brief on an already-brief pane never acks and fails
 * the whole run 1.5 s later with a message about a stalled frontend.
 */
async function setViewMode(pane: 'left' | 'right', mode: 'full' | 'brief'): Promise<void> {
  const current = parsePaneView(await mcpReadResource('cmdr://state?include=panes'), pane)
  if (current === mode) return
  await mcpCall('set_view_mode', { pane, mode })
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

/**
 * Pins the two bits of window chrome that photograph whatever the machine happens to be
 * doing: the title's license suffix, and the repo chip's ahead/behind/dirty state.
 *
 * Both are honest app output that makes a poor master. The title suffix depends on a
 * license the shots instance doesn't have, and the chip reports the working copy's real
 * unpushed-commit count, so the same shot says `main` one day and `main · +14` the next.
 *
 * ❗ This paints over the RENDERED value only. It writes no license, and it must never
 * grow into anything that changes what the app believes: a master is allowed to show a
 * chosen state, never a fake one the app would act on.
 *
 * A `MutationObserver` rather than a one-shot rewrite, because both are reactive: the
 * chip repaints whenever the repo's watcher fires (an agent committing in a sibling
 * worktree is enough), and the title refetches on every license event. Each pass writes
 * only what differs, so the observer settles instead of feeding itself.
 */
async function pinVolatileChrome(page: TauriPage): Promise<void> {
  await page.evaluate(`(() => {
    if (window.__cmdrShotsChromePinned) return
    window.__cmdrShotsChromePinned = true
    const apply = () => {
      const title = document.querySelector('.title-text')
      if (title && title.textContent.trim() !== ${JSON.stringify(MASTER_TITLE)}) {
        title.textContent = ${JSON.stringify(MASTER_TITLE)}
      }
      for (const chip of document.querySelectorAll('.repo-chip')) {
        for (const state of ['dirty', 'ahead', 'behind', 'detached', 'unborn']) {
          if (chip.classList.contains(state)) chip.classList.remove(state)
        }
        if (chip.dataset.state !== 'clean') chip.dataset.state = 'clean'
        // The '·' separator and the '+14 / dirty' suffix are separate spans; dropping
        // them leaves the icon and the branch name, which is the clean-state chip.
        chip.querySelector('.sep')?.remove()
        chip.querySelector('.sub')?.remove()
      }
    }
    new MutationObserver(apply).observe(document.body, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
      attributeFilter: ['class', 'data-state'],
    })
    apply()
  })()`)
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
