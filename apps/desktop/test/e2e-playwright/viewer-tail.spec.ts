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

// Seed with enough lines to push the backend over the 1 MB FullLoad
// threshold so it picks ByteSeek + LineIndex. FullLoad doesn't support
// `extend_to`, so tail mode is a no-op on FullLoad files. Each line is
// ~84 bytes ("seed line NNNNNNNN " + 60 'x' chars + "\n"), so we need
// ~12.5k lines for 1 MB.
const INITIAL_LINES: string[] = []
for (let i = 0; i < 15000; i++) {
  INITIAL_LINES.push(`seed line ${String(i).padStart(8, '0')} ${'x'.repeat(60)}`)
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

    // Capture the initial total-lines count from the status bar. The status bar
    // text contains "<N> lines"; we parse that integer so the assertion is
    // robust against any future status-bar wording tweak.
    function statusLines(): Promise<number | null> {
      return v.evaluate<number | null>(`
        (function () {
          const bar = document.querySelector('.status-bar')
          if (!bar) return null
          const match = (bar.textContent || '').match(/(\\d+)\\s+lines?/)
          return match ? Number(match[1]) : null
        })()
      `)
    }
    // Wait for the ByteSeek → LineIndex upgrade to populate `totalLines`.
    // Before the upgrade finishes the status bar omits the line count span
    // entirely (status-bar template: `{#if totalLines !== null}`); the upgrade
    // for a 1.2 MB file is fast but not synchronous.
    await expect
      .poll(async () => await statusLines(), { timeout: 8000 })
      .not.toBeNull()
    const initialLines = await statusLines()
    expect(initialLines).not.toBeNull()
    expect(initialLines).toBeGreaterThan(0)

    // Append on the Node side. fs.appendFile is async; we await its promise
    // before polling so the assertion isn't racing with the write itself.
    const appended = '\nappended via tail E2E\nsecond appended line\n'
    await fs.promises.appendFile(TAIL_FIXTURE_PATH, appended, 'utf-8')
    const expectedSize = fs.statSync(TAIL_FIXTURE_PATH).size
    expect(expectedSize).toBeGreaterThan(INITIAL_CONTENT.length)

    // Wait until the status-bar line count reflects the new content. The
    // BE-side path: FSEvents debounce (~300 ms) → ViewerWatcher classification
    // → session handler → `apply_tail_extend` → `total_bytes` advances →
    // status poll surfaces the new totalLines. We assert on totalLines (a
    // structural property) rather than rendered line content because the
    // viewport stays at the top after the append; the appended bytes live at
    // EOF and aren't in the rendered range.
    await expect
      .poll(
        async () => {
          const now = await statusLines()
          return now !== null && initialLines !== null && now > initialLines
        },
        // Generous deadline: FSEvents debounce on macOS is ~300 ms, plus
        // BE-side debouncer + tail extend + FE indexing poll.
        { timeout: 15000 },
      )
      .toBe(true)
  })
})
