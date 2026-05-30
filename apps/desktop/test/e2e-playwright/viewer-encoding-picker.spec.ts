/**
 * Cross-component E2E for the viewer encoding picker.
 *
 * Per the milestone 2 plan, the only E2E spec for the encoding work is the
 * one that needs the real Tauri window-overlay layout: a 6 MB UTF-16 LE
 * fixture, opened in the viewer; switching encoding from UTF-16 LE to UTF-8
 * must trigger a backend rebuild while the viewport stays interactive. The
 * other behaviour (component rendering, a11y, IPC contract, drain-and-swap)
 * is covered by vitest and Rust unit tests.
 */

import fs from 'fs'
import os from 'os'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const ENC_FIXTURE_DIR = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-viewer-enc-'))
const ENC_FIXTURE_PATH = path.join(ENC_FIXTURE_DIR, 'six-mb-utf16le.txt')

/**
 * Build a 6 MB UTF-16 LE text fixture: roughly 250k repetitions of
 * "hello world\n", which lands at ~6.0 MB once UTF-16 LE encoding (2 bytes
 * per ASCII char) doubles the bytes. The line count is large enough that the
 * backend picks LineIndex; the byte size is large enough that the rebuild
 * runs in the background instead of completing synchronously.
 */
function buildSixMbUtf16Le(): Buffer {
  const line = 'hello world\n'
  const buf = Buffer.alloc(line.length * 2)
  for (let i = 0; i < line.length; i++) {
    buf.writeUInt16LE(line.charCodeAt(i), i * 2)
  }
  const targetBytes = 6 * 1024 * 1024
  const repeats = Math.ceil(targetBytes / buf.length)
  return Buffer.concat(Array.from({ length: repeats }, () => buf))
}

async function openViewerForFile(mainPage: TauriPage, filePath: string): Promise<TauriPage> {
  const viewer = await openViewerWindow(mainPage, filePath)
  await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 15000)
  return viewer
}

test.describe('File viewer encoding picker', () => {
  let viewer: TauriPage
  let viewerLabel: string

  test.beforeAll(() => {
    fs.writeFileSync(ENC_FIXTURE_PATH, buildSixMbUtf16Le())
  })

  test.afterAll(() => {
    try {
      fs.rmSync(ENC_FIXTURE_DIR, { recursive: true, force: true })
    } catch {
      // best-effort cleanup
    }
  })

  test.beforeEach(async ({ tauriPage }) => {
    viewer = await openViewerForFile(tauriPage as TauriPage, ENC_FIXTURE_PATH)
    const wl = viewer.targetWindow
    if (!wl) throw new Error('Scoped viewer page has no targetWindow label')
    viewerLabel = wl
  })

  test.afterEach(async ({ tauriPage }) => {
    await closeScopedWindow(tauriPage as TauriPage, viewer, viewerLabel)
  })

  test('switching encoding on a 6 MB file keeps the viewport interactive', async () => {
    // Wait for the encoding picker to appear with UTF-16 LE selected (detected).
    await viewer.waitForSelector('select.encoding-picker', 8000)

    await expect
      .poll(
        async () => {
          const selected = await viewer.evaluate<string | null>(`
            (function () {
              const picker = document.querySelector('select.encoding-picker')
              return picker ? picker.value : null
            })()
          `)
          return selected
        },
        { timeout: 8000 },
      )
      .toBe('utf16Le')

    // Verify the detected suffix shows on the UTF-16 LE row.
    const detectedLabel = await viewer.evaluate<string | null>(`
      (function () {
        const opt = document.querySelector('select.encoding-picker option[value="utf16Le"]')
        return opt ? opt.textContent : null
      })()
    `)
    expect(detectedLabel).toContain('(Detected)')

    // Switch to UTF-8: triggers a non-instant rebuild (UTF-16 -> UTF-8 changes
    // byte layout). The picker reflects the new selection immediately; the
    // rebuild runs in the background.
    await viewer.evaluate(`
      (function () {
        const picker = document.querySelector('select.encoding-picker')
        picker.value = 'utf8'
        picker.dispatchEvent(new Event('change', { bubbles: true }))
      })()
    `)

    await expect
      .poll(
        async () => {
          return await viewer.evaluate<string | null>(`
            (function () {
              const picker = document.querySelector('select.encoding-picker')
              return picker ? picker.value : null
            })()
          `)
        },
        { timeout: 3000 },
      )
      .toBe('utf8')

    // The viewport must stay interactive during the rebuild: setting scrollTop
    // must take. Re-apply the gesture on every poll iteration rather than once
    // up front — mid-rebuild the virtualized content can briefly have no
    // scrollable height (scrollHeight <= clientHeight clamps scrollTop back to
    // 0), so a single pre-poll set could latch a permanent 0 on a slow host
    // (the Docker flake). Once the rows are back the assignment sticks and
    // scrollTop reads > 0.
    await expect
      .poll(
        async () => {
          return await viewer.evaluate<number | null>(`
            (function () {
              const content = document.querySelector('.file-content')
              if (!content) return null
              content.scrollTop = 400
              return content.scrollTop
            })()
          `)
        },
        { timeout: 3000 },
      )
      .toBeGreaterThan(0)

    // Eventually the rebuild finishes (the picker comes off "disabled" and the
    // toolbar status removes the "Reindexing…" label, which we use as the
    // completion signal).
    await expect
      .poll(
        async () => {
          return await viewer.evaluate<boolean>(`
            (function () {
              const indexing = document.querySelector('.viewer-toolbar-indexing')
              return indexing === null
            })()
          `)
        },
        { timeout: 20000 },
      )
      .toBe(true)
  })
})
