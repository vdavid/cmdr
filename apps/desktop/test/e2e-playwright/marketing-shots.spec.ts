/**
 * The marketing capture: the brand masters in `brand/screenshots/`, plus the pane
 * rectangles the website hero is cut from.
 *
 * Driven by `pnpm marketing:shots`, never by a bare Playwright run: the orchestrator
 * is what launches a prod-looking app on the persistent shots data dir and hands this
 * spec the app's pid. It is a screenshot driver, not a pass/fail suite, so it has its
 * own shard and never joins `all` / `mtp` / `non-mtp`.
 *
 * ❗ It runs with NO fixture tree, photographing real folders, and therefore on
 * `captureTest` (no leak guard). Read
 * `docs/specs/marketing-screenshot-pipeline-plan.md` before changing how it is wired.
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { captureTest as test, expect } from './fixtures.js'
import { insetRect } from './marketing-shots-frame.js'
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

// Staging a master takes several UI round-trips plus up to three shots, and the
// config's 15 s default would cut the first one off. Generous on purpose: on timeout
// Playwright destroys the plugin socket, so every later shot fails with `Not
// connected`, which reads like a crash and buries the real message.
test.setTimeout(180_000)

test.describe('marketing masters', () => {
  test.skip(process.platform !== 'darwin', 'The masters are macOS window shots: traffic lights and a system shadow.')

  test('main window, dark and light', async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    await page.waitForSelector('.file-pane', 15000)

    await setWindowSize(page, 'main', MAIN_WINDOW.width, MAIN_WINDOW.height)
    await expect
      .poll(async () => (await windowMetrics(page, 'main')).logical.width, { timeout: 5000 })
      .toBe(MAIN_WINDOW.width)

    const metrics = await windowMetrics(page, 'main')
    // Every margin this pipeline gates on is a device-pixel number, so a 1x display
    // has to fail saying that rather than failing at arithmetic that looks broken.
    expect(metrics.scale, 'the masters are retina shots; run this on a 2x display').toBe(2)

    await setTheme(page, 'dark')
    await shootWithShadow(page, 'main', 'app-main-dark.png', metrics)

    // Measure the hero cutouts from the SAME staged window that was just photographed.
    // Measuring separately is exactly how the committed rectangles drifted a redesign
    // behind the shot they were supposed to describe.
    const cutouts = await measurePaneCutouts(page, metrics.scale)

    await setTheme(page, 'light')
    await shootWithShadow(page, 'main', 'app-main-light.png', metrics)

    writeFileSync(
      join(outputDir(), 'hero-cutouts.json'),
      `${JSON.stringify(
        {
          measuredWith: 'apps/desktop/test/e2e-playwright/marketing-shots.spec.ts, on the live DOM',
          window: metrics.device,
          panes: { left: cutouts[0], right: cutouts[1] },
        },
        null,
        2,
      )}\n`,
    )

    await setTheme(page, 'dark')
  })
})

/**
 * Changes a setting the way the app's own MCP server does: emit `mcp-set-setting` and
 * let the main window's bridge apply it (`settings/mcp-main-bridge.ts`).
 *
 * ❗ Not `invoke('set_setting')` — there is no such Tauri command; the MCP tool is a
 * round-trip THROUGH the frontend. And not a direct CSS or `@tauri-apps/api/app`
 * poke either: those change how the app looks without changing what it thinks, so the
 * settings master would photograph a radio button disagreeing with the window around
 * it.
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
    .poll(
      async () =>
        page.evaluate<boolean>(`window.matchMedia('(prefers-color-scheme: dark)').matches`).then((dark) => dark),
      { timeout: 10000 },
    )
    .toBe(mode === 'dark')
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
