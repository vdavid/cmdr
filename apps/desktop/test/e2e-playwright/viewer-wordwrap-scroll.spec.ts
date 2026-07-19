/**
 * Regression E2E for word-wrap virtual scrolling.
 *
 * The wrap-mode height map (pretext) computes per-line wrapped heights at the
 * measured available text width. That width must come from the row geometry
 * (scroll container minus gutter and padding), NOT from the first rendered
 * line's text span: the span shrink-wraps to its content, so a file whose
 * FIRST line is short ("# Cmdr") once produced a ~44px wrap width, a ~7x
 * inflated scroll height, and a viewer where everything past ~line 60 was
 * unreachable blank space.
 *
 * The user-level contract pinned here: with word wrap on, scrolling to the
 * bottom shows the last line of the file. It must hold in BOTH height modes
 * (pretext map and the averaged fallback), so the test doesn't gate on the
 * height map being ready; it re-scrolls and re-checks across a short window
 * that comfortably covers the map flipping ready (<100 ms for a file this
 * size).
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow, sleep } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const fixtureRoot = (() => {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root)
    throw new Error('CMDR_E2E_START_PATH env var is not set; fixtures must be created before running this spec')
  return root
})()

// Short first line + long wrappable lines below: the exact shape that broke.
const testFilePath = path.join(fixtureRoot, 'left', 'short-first-line.md')
const longLine = 'All work and no play makes the height map a dull boy. '.repeat(4).trim()
const fileContent = `# T\n\n${`${longLine}\n`.repeat(120)}`
// FullLoad line count: one line per newline, plus the trailing empty line
// (the file ends with a newline).
const lastLineSelector = `.line[data-line="${String(fileContent.split('\n').length - 1)}"]`

// Binary fixture with highly variable wrapped line heights, the shape the height
// map must get right or the virtual scroll drifts (blank gaps, last line
// unreachable). Three line kinds cycle:
//  - long lines of space-separated tokens carrying control bytes (0x01-0x06):
//    wrap to several rows, AND a canvas `measureText` predictor mis-measures the
//    control-byte advances, so a predictor drifts where DOM measurement holds;
//  - an unbreakable no-space run ('W'*200): the real flex `.line-text` renders
//    it on ONE overflowing row, so the measurer must NOT wrap it (guards the
//    flex-layout replication in `measureLineHeightsViaDom`);
//  - a short line: one row.
const binaryFilePath = path.join(fixtureRoot, 'left', 'binary-drift.bin')
const binaryContent: Buffer = (() => {
  const rows: Buffer[] = []
  for (let i = 0; i < 300; i++) {
    if (i % 3 === 0) {
      const tokens: string[] = []
      for (let w = 0; w < 40; w++) tokens.push('abc' + String.fromCharCode(1 + (w % 6)) + 'xy')
      rows.push(Buffer.from(tokens.join(' '), 'latin1')) // long, wraps to several rows
    } else if (i % 3 === 1) {
      rows.push(Buffer.from('W'.repeat(200), 'latin1')) // no-space run: one overflowing row
    } else {
      rows.push(Buffer.from('short', 'latin1')) // one row
    }
  }
  return Buffer.concat(rows.flatMap((r) => [r, Buffer.from([0x0a])]))
})()
// 300 newlines => content lines 0..299 plus a trailing empty line 300.
const binaryLastLineSelector = `.line[data-line="300"]`

/** Scrolls to a fraction of the file and reports whether rendered content fills
 *  down to the viewport bottom. With a drifting height map, mid-file scrolling
 *  leaves a blank band below the last placed line. Scroll + check in one
 *  evaluate so each poll iteration re-scrolls and lets the virtualizer settle. */
function scrollToFractionAndCheckFilled(viewer: TauriPage, fraction: number): Promise<string> {
  return viewer.evaluate<string>(`
    (function () {
      const content = document.querySelector('.file-content')
      if (!content) return 'no-content'
      content.scrollTop = Math.round((content.scrollHeight - content.clientHeight) * ${String(fraction)})
      const c = content.getBoundingClientRect()
      const lines = document.querySelectorAll('.line')
      if (lines.length === 0) return 'no-lines'
      let maxBottom = -Infinity
      for (const l of lines) {
        const b = l.getBoundingClientRect().bottom; if (b > maxBottom) maxBottom = b
      }
      // Content must reach within ~2 rows of the viewport bottom (no blank band).
      return maxBottom >= c.bottom - 40 ? 'filled' : ('gap:' + String(Math.round(c.bottom - maxBottom)))
    })()
  `)
}

/** Dispatches an unmodified `w` keydown on the viewer window (the production
 *  word-wrap toggle binding, handled by the `<svelte:window>` listener). */
async function pressWrapToggle(viewer: TauriPage): Promise<void> {
  await viewer.evaluate(`window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', bubbles: true }))`)
}

/** True when the status bar shows the "wrap" badge (the word-wrap indicator). */
function wrapBadgeVisible(viewer: TauriPage): Promise<boolean> {
  return viewer.evaluate<boolean>(`
    (function () {
      const badges = document.querySelectorAll('.status-bar .backend-badge')
      for (const badge of badges) {
        if ((badge.textContent || '').trim() === 'wrap') return true
      }
      return false
    })()
  `)
}

/** Brings word wrap to the desired state regardless of the persisted setting
 *  (a previously failed run can leave wrap toggled on). */
async function setWordWrap(viewer: TauriPage, on: boolean): Promise<void> {
  if ((await wrapBadgeVisible(viewer)) === on) return
  await pressWrapToggle(viewer)
  await expect.poll(() => wrapBadgeVisible(viewer), { timeout: 3000 }).toBe(on)
}

/** Scrolls the viewer to the very bottom, then reports whether the last line of
 *  the file is rendered inside the viewport. Scroll + check in one evaluate so
 *  every poll iteration re-scrolls (the spacer height can change while the
 *  height map prepares). */
function scrollToBottomAndCheckLastLineVisible(
  viewer: TauriPage,
  selector: string = lastLineSelector,
): Promise<string> {
  return viewer.evaluate<string>(`
    (function () {
      const content = document.querySelector('.file-content')
      if (!content) return 'no-content'
      content.scrollTop = content.scrollHeight
      const target = document.querySelector('${selector}')
      if (!target) return 'last-line-not-rendered'
      const c = content.getBoundingClientRect()
      const r = target.getBoundingClientRect()
      return r.bottom > c.top && r.top < c.bottom ? 'visible' : 'rendered-but-outside-viewport'
    })()
  `)
}

test.describe('Viewer word-wrap scrolling', () => {
  test('with wrap on, scrolling to the bottom shows the end of a short-first-line file', async ({ tauriPage }) => {
    const mainPage = tauriPage as TauriPage
    fs.writeFileSync(testFilePath, fileContent)

    const viewer = await openViewerWindow(mainPage, testFilePath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)

      await setWordWrap(viewer, true)

      // Reach the bottom (covers the averaged-height phase and the map flip).
      await expect.poll(() => scrollToBottomAndCheckLastLineVisible(viewer), { timeout: 10000 }).toBe('visible')

      // Paranoia window: the height map readies asynchronously; the end of the
      // file must STAY reachable after it kicks in, not just during the
      // averaged-height phase. Five samples, 100 ms apart. The sleep is NOT a
      // readiness wait: we're asserting a condition HOLDS over time (there's no
      // DOM signal for "the height map flipped ready"), so a probe can't
      // replace it.
      for (let i = 0; i < 5; i++) {
        // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- sampling interval of a stability assertion, not a readiness wait
        await sleep(100)
        expect(await scrollToBottomAndCheckLastLineVisible(viewer)).toBe('visible')
      }

      // Cleanup half 1: wrap back off so the persisted setting returns to its
      // default for later specs.
      await setWordWrap(viewer, false)
    } finally {
      await closeScopedWindow(mainPage, viewer, label)
      fs.rmSync(testFilePath, { force: true })
    }
  })

  test('with wrap on, a control-byte binary file scrolls without drift (DOM-measured heights)', async ({
    tauriPage,
  }) => {
    const mainPage = tauriPage as TauriPage
    fs.writeFileSync(binaryFilePath, binaryContent)

    const viewer = await openViewerWindow(mainPage, binaryFilePath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)
      await setWordWrap(viewer, true)

      // Mid-file, content must fill the viewport (a drifting height map leaves a
      // blank band below the last placed line). Poll so the height map's async
      // measure pass has flipped ready and the virtualizer has settled.
      await expect.poll(() => scrollToFractionAndCheckFilled(viewer, 0.5), { timeout: 10000 }).toBe('filled')

      // And the end of the file stays reachable (a wrong total height puts the
      // last line off-screen). Sample across the ready flip.
      await expect
        .poll(() => scrollToBottomAndCheckLastLineVisible(viewer, binaryLastLineSelector), { timeout: 10000 })
        .toBe('visible')
      for (let i = 0; i < 5; i++) {
        // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- sampling interval of a stability assertion, not a readiness wait
        await sleep(100)
        expect(await scrollToBottomAndCheckLastLineVisible(viewer, binaryLastLineSelector)).toBe('visible')
      }

      await setWordWrap(viewer, false)
    } finally {
      await closeScopedWindow(mainPage, viewer, label)
      fs.rmSync(binaryFilePath, { force: true })
    }
  })
})
