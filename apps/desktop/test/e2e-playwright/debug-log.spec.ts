/**
 * E2E for Help > View debug log: the command opens the app's own `cmdr.log` in a viewer that
 * starts tailed, parked at the end, and stays at the end as the log grows.
 *
 * Only an E2E proves the chain end to end: the backend's resolved log path (the harness's
 * isolated data dir), the `tail=1` hand-off to the viewer window, and the follow on a real
 * filesystem append. The log is usually under 1 MB here, so this also covers tail mode on the
 * FullLoad backend.
 */

import fs from 'fs'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, dispatchMenuCommand } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

test.describe('Help > View debug log', () => {
  // FSEvents debounce plus the tail extend plus the FE refetch, on a loaded CI box.
  test.describe.configure({ timeout: waitBudget(30000) })

  test('opens the live log tailed, at the end, and follows an append', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const before = new Set((await main.listWindows()).map((w) => w.label).filter((l) => l.startsWith('viewer-')))
    await dispatchMenuCommand(main, 'help.viewDebugLog')
    const viewer = await main.waitForWindow((w) => w.label.startsWith('viewer-') && !before.has(w.label), {
      timeout: waitBudget(10000),
    })
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', waitBudget(10000))

      // The path the backend resolved rides the viewer's URL: the harness's own log file.
      const logPath = await viewer.evaluate<string | null>(`new URLSearchParams(location.search).get('path')`)
      expect(logPath).toMatch(/[/\\]cmdr\.log$/)
      if (!logPath) throw new Error('viewer opened without a path')

      await expect
        .poll(
          async () =>
            await viewer.evaluate<string | null>(
              `document.querySelector('button[aria-label^="Tail mode"]')?.getAttribute('aria-checked') ?? null`,
            ),
          { timeout: waitBudget(3000) },
        )
        .toBe('true')

      // The app writes to its own log the whole time, so "at the end" is the one stable read.
      const atEnd = `(function () {
        const el = document.querySelector('.file-content')
        if (!el) return false
        return el.scrollHeight - el.clientHeight - el.scrollTop <= 40
      })()`
      await expect.poll(async () => await viewer.evaluate<boolean>(atEnd), { timeout: waitBudget(5000) }).toBe(true)

      const marker = `debug-log e2e marker ${String(Date.now())}`
      await fs.promises.appendFile(logPath, `${marker}\n`, 'utf-8')
      await expect
        .poll(
          async () =>
            await viewer.evaluate<boolean>(
              `(document.querySelector('.file-content')?.textContent ?? '').includes(${JSON.stringify(marker)})`,
            ),
          { timeout: waitBudget(15000) },
        )
        .toBe(true)
    } finally {
      await closeScopedWindow(main, viewer, label)
    }
  })
})
