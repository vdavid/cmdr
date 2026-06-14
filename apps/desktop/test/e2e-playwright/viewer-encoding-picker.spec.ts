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
    // Wait for the encoding picker (Ark `ui/Select`) to appear with UTF-16 LE
    // selected (detected). The selected option carries `aria-selected="true"`;
    // its `data-value` is the encoding id.
    await viewer.waitForSelector('.viewer-toolbar-pickers .select-trigger', 8000)

    // Scope every item lookup to the encoding picker's own listbox. The toolbar holds
    // two `ui/Select` pickers: the disabled view-mode picker (a single "text" item)
    // first, the encoding picker second. On webkit2gtk the disabled picker's item is
    // mounted AND carries `aria-selected="true"`, so an unscoped
    // `.viewer-toolbar-pickers [data-part="item"]` query matches it first and reads
    // "text" instead of the detected encoding (passes on macOS, where that item isn't
    // matched). The encoding picker is the last `.select-content` (mirrors the
    // last-trigger click below).
    const inEncodingPicker = (innerSelector: string): string => `
      (function () {
        const contents = document.querySelectorAll('.viewer-toolbar-pickers .select-content')
        const enc = contents[contents.length - 1]
        return enc ? enc.querySelector('${innerSelector}') : null
      })()
    `

    const selectedValue = `
      (function () {
        const opt = ${inEncodingPicker('[data-part="item"][aria-selected="true"]')}
        return opt ? opt.getAttribute('data-value') : null
      })()
    `

    await expect.poll(async () => viewer.evaluate<string | null>(selectedValue), { timeout: 8000 }).toBe('utf16Le')

    // Verify the detected suffix shows on the UTF-16 LE row.
    const detectedLabel = await viewer.evaluate<string | null>(`
      (function () {
        const opt = ${inEncodingPicker('[data-part="item"][data-value="utf16Le"]')}
        return opt ? opt.textContent : null
      })()
    `)
    expect(detectedLabel).toContain('(Detected)')

    // Switch to UTF-8: triggers a non-instant rebuild (UTF-16 -> UTF-8 changes
    // byte layout). Open the listbox, then click the UTF-8 option. The picker
    // reflects the new selection immediately; the rebuild runs in the
    // background. The encoding picker is the second trigger in the toolbar (the
    // first is the view-mode picker, which is disabled).
    // A bare `.click()` doesn't drive an Ark/zag `Select` on webkit2gtk: the trigger
    // toggles on `pointerdown` and items select on `pointerup`, neither of which a
    // synthetic click fires (it works on macOS WebKit by luck). Drive a realistic
    // pointer+mouse sequence instead, the same shape the file-list specs use.
    const fireClick = (elExpr: string): string => `
      (function () {
        const el = ${elExpr}
        if (!el) return false
        const r = el.getBoundingClientRect()
        const o = {
          bubbles: true,
          cancelable: true,
          composed: true,
          button: 0,
          clientX: r.left + r.width / 2,
          clientY: r.top + r.height / 2,
          pointerId: 1,
          pointerType: 'mouse',
          isPrimary: true,
        }
        el.dispatchEvent(new PointerEvent('pointerdown', o))
        el.dispatchEvent(new MouseEvent('mousedown', o))
        el.dispatchEvent(new PointerEvent('pointerup', o))
        el.dispatchEvent(new MouseEvent('mouseup', o))
        el.dispatchEvent(new MouseEvent('click', o))
        return true
      })()
    `

    const encodingTriggerExpr = `
      (function () {
        const triggers = document.querySelectorAll('.viewer-toolbar-pickers .select-trigger')
        return triggers[triggers.length - 1]
      })()
    `

    // The encoding picker is `disabled={isIndexing}`. On a slow host the initial
    // LineIndex build of the 6 MB file can still be running here, and a disabled
    // trigger won't open no matter the gesture, so wait for it to enable first.
    await expect
      .poll(
        async () =>
          viewer.evaluate<boolean>(`
            (function () {
              const triggers = document.querySelectorAll('.viewer-toolbar-pickers .select-trigger')
              const trig = triggers[triggers.length - 1]
              return !!trig && !trig.disabled && trig.getAttribute('data-disabled') === null
            })()
          `),
        { timeout: 20000 },
      )
      .toBe(true)

    // Open the encoding listbox.
    await viewer.evaluate(fireClick(encodingTriggerExpr))

    // Wait for Ark to report the listbox open via its own `data-state`, rather than an
    // `offsetParent` visibility probe (offsetParent is null for the floating-positioned
    // content on webkit2gtk even when it's visible).
    await expect
      .poll(
        async () =>
          viewer.evaluate<string | null>(`
            (function () {
              const contents = document.querySelectorAll('.viewer-toolbar-pickers .select-content')
              const enc = contents[contents.length - 1]
              return enc ? enc.getAttribute('data-state') : null
            })()
          `),
        { timeout: 5000 },
      )
      .toBe('open')

    // Pick UTF-8.
    await viewer.evaluate(fireClick(inEncodingPicker('[data-part="item"][data-value="utf8"]')))

    await expect.poll(async () => viewer.evaluate<string | null>(selectedValue), { timeout: 3000 }).toBe('utf8')

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
