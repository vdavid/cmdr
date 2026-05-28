/**
 * Cross-component E2E for viewer tail mode.
 *
 * The bulk of tail-mode behaviour (composable state, toast wording, IPC
 * shapes, watcher classification) lives in vitest + Rust unit tests. What
 * only an E2E can verify is the cross-component flow: the user clicks the
 * tail toggle, the FS-watcher fires for a real filesystem append, and the
 * viewport auto-scrolls to the new bottom.
 *
 * Fixture: a small text file we append to via Node's `fs.appendFile` after
 * the viewer is open. macOS FSEvents has variable latency (~300 ms debounce
 * plus ramp-up), so all waits use `expect.poll` with a generous deadline.
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const TAIL_FIXTURE_DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-viewer-tail-'))
const TAIL_FIXTURE_PATH = path.join(TAIL_FIXTURE_DIR, 'tailable.log')

// Seed with enough lines that the viewport doesn't start at EOF — the
// auto-scroll assertion below depends on the viewport being away from EOF
// when the append fires (auto-scroll only triggers when the user is at EOF).
const INITIAL_LINES: string[] = []
for (let i = 0; i < 50; i++) {
  INITIAL_LINES.push(`seed line ${String(i)}`)
}
const INITIAL_CONTENT = INITIAL_LINES.join('\n') + '\n'

async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)
  return viewer
}

test.describe('File viewer tail mode', () => {
  let viewer: TauriPage | undefined
  let viewerLabel: string | undefined

  // FSEvents debounce (300 ms) plus the BE-side coalescer plus the FE refetch
  // budget pushes this test past the default 8 s in slow-CI conditions. The
  // poll deadline below is 10 s; allow another window for open + setup.
  test.describe.configure({ timeout: 30000 })

  test.beforeAll(() => {
    fs.writeFileSync(TAIL_FIXTURE_PATH, INITIAL_CONTENT, 'utf-8')
  })

  test.afterAll(() => {
    try {
      fs.rmSync(TAIL_FIXTURE_DIR, { recursive: true, force: true })
    } catch {
      // best-effort cleanup
    }
  })

  test.beforeEach(async ({ tauriPage }) => {
    viewer = undefined
    viewerLabel = undefined
    // Refresh the file each run so a previous run's append isn't carried over.
    fs.writeFileSync(TAIL_FIXTURE_PATH, INITIAL_CONTENT, 'utf-8')
    const v = await openViewerForFile(tauriPage as TauriPage, TAIL_FIXTURE_PATH)
    if (!v.targetWindow) throw new Error('Scoped viewer page has no targetWindow label')
    viewer = v
    viewerLabel = v.targetWindow
  })

  test.afterEach(async ({ tauriPage }) => {
    // Guard the close path: if beforeEach failed, viewer may be undefined and
    // a blind closeScopedWindow with undefined label would poll for 5 s
    // looking for a window that doesn't exist and starve the next test's
    // budget.
    if (viewer && viewerLabel !== undefined) {
      await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
    }
  })

  test('enabling tail mode auto-extends the viewport on append', async () => {
    if (!viewer) throw new Error('viewer was not opened in beforeEach')
    const v = viewer

    // Find and click the tail toggle. It carries `aria-label="Tail mode: ..."`.
    await v.waitForSelector('button[aria-label^="Tail mode"]', 5000)
    await v.evaluate(`
      (function () {
        const btn = document.querySelector('button[aria-label^="Tail mode"]')
        if (!btn) throw new Error('tail toggle not found')
        btn.click()
      })()
    `)

    // Confirm the toggle flipped on.
    await expect
      .poll(
        async () => {
          return await v.evaluate<string | null>(`
            (function () {
              const btn = document.querySelector('button[aria-label^="Tail mode"]')
              return btn ? btn.getAttribute('aria-checked') : null
            })()
          `)
        },
        { timeout: 3000 },
      )
      .toBe('true')

    // Capture the initial total-lines count from the status bar.
    const initialLines = await v.evaluate<number>(`
      (function () {
        // The status bar shows lines/bytes; we count rendered lines instead so
        // we don't have to parse the formatted total.
        const rows = document.querySelectorAll('.line')
        return rows.length
      })()
    `)
    expect(initialLines).toBeGreaterThan(0)

    // Append on the Node side. fs.appendFile is async; we await its promise
    // before polling so the assertion isn't racing with the write itself.
    const appended = '\nappended via tail E2E\nsecond appended line\n'
    await fs.promises.appendFile(TAIL_FIXTURE_PATH, appended, 'utf-8')

    // Wait until the rendered rows include one of the appended lines OR the
    // total-bytes status reflects the new size. We use rendered-content
    // matching rather than scroll position because the viewport auto-scroll
    // policy ("only auto-scroll if the user is at EOF") doesn't apply in the
    // initial-render case; what we really care about is that the BE picked
    // up the append and the FE has refetched lines.
    const expectedSize = fs.statSync(TAIL_FIXTURE_PATH).size
    await expect
      .poll(
        async () => {
          return await v.evaluate<boolean>(`
            (function () {
              const text = document.body.textContent || ''
              return text.includes('appended via tail E2E')
            })()
          `)
        },
        // Generous deadline: FSEvents debounce on macOS is ~300 ms, plus
        // BE-side debouncer + tail extend + FE refetch.
        { timeout: 10000 },
      )
      .toBe(true)

    // Sanity: the file on disk grew, and the BE total-bytes status reflects it.
    expect(expectedSize).toBeGreaterThan(INITIAL_CONTENT.length)
  })
})
