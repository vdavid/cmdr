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

import { waitBudget } from './wait-budget.js'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { closeScopedWindow, openViewerWindow, PRIMARY_KEY_FLAG, sleep } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const fixtureRoot = (() => {
  const root = process.env.CMDR_E2E_START_PATH
  if (!root)
    throw new Error('CMDR_E2E_START_PATH env var is not set; fixtures must be created before running this spec')
  return root
})()

// Short first line + long wrappable lines below: the exact shape that broke.
const testFilePath = path.join(fixtureRoot, 'left', 'short-first-line.txt')
const longLine = 'All work and no play makes the height map a dull boy. '.repeat(4).trim()
const fileContent = `# T\n\n${`${longLine}\n`.repeat(120)}`
// One row per newline: the file ends with one, and no empty row follows a final
// newline, so the last row is the last line of text.
const lastLineSelector = `.line[data-row="${String(fileContent.split('\n').length - 2)}"]`

// Binary fixture with highly variable wrapped line heights, the shape the height
// map must get right or the virtual scroll drifts (blank gaps, last line
// unreachable). Three line kinds cycle:
//  - long lines of space-separated tokens carrying control bytes (0x01-0x06):
//    wrap to several rows, AND a canvas `measureText` predictor mis-measures the
//    control-byte advances, so a predictor drifts where DOM measurement holds;
//  - an unbreakable no-space run ('W'*200): `.line-text`'s `min-width:0` lets
//    break-word wrap it to fit, so the measurer must wrap it the SAME way (guards
//    that the flex-layout probe in `measureLineHeightsViaDom` mirrors `.line`);
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
// 300 newline-ended lines => rows 0..299, with no empty row after the final newline.
const binaryLastLineSelector = `.line[data-row="299"]`

// One unbreakable no-space line (cycling digits, so a slice's position is visible
// in its content) long enough to wrap onto several visual rows at any window width.
const noSpaceFilePath = path.join(fixtureRoot, 'left', 'wrap-drag-no-space.txt')
const noSpaceLine = '0123456789'.repeat(60)

interface WrapDragResult {
  /** Visual rows the line wrapped onto, counted off the character boxes. */
  visualRows: number
  /** Px between the first visual row's glyph boxes and the second's: the leading no box covers. */
  leadingPx: number
  /** The line offsets the drag's two points sit over, read off the character boxes. */
  from: number
  to: number
}

/**
 * Drags along row 0's wrapped line with synthetic pointer events, 10 px to 50 px into a
 * visual row, and returns the offsets under both points (measured by the test itself,
 * one `Range` per character), which is the slice the copy must hold.
 *
 * - `across-wrap`: from the first visual row's glyphs to the second's, so the selection
 *   crosses a wrap point.
 * - `in-leading`: both points in the leading under the first visual row (a quarter of
 *   the way into the band, so nearer the first row), where no character box reaches.
 *   That's where #962's drag sat: it resolved both ends to the wrap point.
 */
function dragOnWrappedRow(viewer: TauriPage, mode: 'across-wrap' | 'in-leading'): Promise<WrapDragResult> {
  return viewer.evaluate<WrapDragResult>(`
    (function () {
      const mode = ${JSON.stringify(mode)}
      const line = document.querySelector('[data-row="0"] .line-text')
      if (!line) throw new Error('row 0 not found')
      // The previous drag's selection paint splits the text across spans, so walk every
      // text node (Svelte's comment anchors sit between them).
      const tops = []
      const boxes = []
      const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT)
      for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
        for (let i = 0; i < node.nodeValue.length; i++) {
          const r = document.createRange()
          r.setStart(node, i); r.setEnd(node, i + 1)
          const rects = Array.from(r.getClientRects()).filter((b) => b.width > 0)
          const b = rects[0] || r.getBoundingClientRect()
          boxes.push(b)
          if (!tops.some((t) => Math.abs(t - b.top) < 1)) tops.push(b.top)
        }
      }
      const n = boxes.length
      tops.sort((a, b) => a - b)
      if (tops.length < 2) throw new Error('row 0 did not wrap: ' + String(tops.length) + ' visual row(s)')
      const rowBox = (top) => boxes.filter((b) => Math.abs(b.top - top) < 1)
      const first = rowBox(tops[0])
      const second = rowBox(tops[1])
      const leadingPx = second[0].top - first[0].bottom
      // The two modes press 10 px apart, past the multi-click slop, so a run of both
      // inside the double-click interval doesn't turn the second press into a word pick.
      const startX = first[0].left + (mode === 'across-wrap' ? 10 : 20)
      const endX = (mode === 'across-wrap' ? second : first)[0].left + 50
      // Half a px up from a quarter into the band: inside it on any real leading, and
      // still on the first row's glyphs where a webview leaves none.
      const leadingY = first[0].bottom + leadingPx / 4 - 0.5
      const startY = mode === 'across-wrap' ? (first[0].top + first[0].bottom) / 2 : leadingY
      const endY = mode === 'across-wrap' ? (second[0].top + second[0].bottom) / 2 : leadingY
      const endTop = mode === 'across-wrap' ? tops[1] : tops[0]
      // The offset whose caret position is nearest a point on a visual row.
      const offsetAt = (x, top) => {
        let best = 0, bestDist = Infinity
        for (let i = 0; i < n; i++) {
          if (Math.abs(boxes[i].top - top) >= 1) continue
          for (const [edge, off] of [[boxes[i].left, i], [boxes[i].right, i + 1]]) {
            const d = Math.abs(edge - x)
            if (d < bestDist) { bestDist = d; best = off }
          }
        }
        return best
      }
      const target = document.querySelector('.file-content')
      if (!target) throw new Error('file-content not found')
      function fire(type, x, y) {
        target.dispatchEvent(new PointerEvent(type, {
          bubbles: true, cancelable: true, clientX: x, clientY: y,
          button: 0, pointerId: 1, pointerType: 'mouse',
        }))
      }
      fire('pointerdown', startX, startY)
      fire('pointermove', endX, endY)
      fire('pointerup', endX, endY)
      return { visualRows: tops.length, leadingPx, from: offsetAt(startX, tops[0]), to: offsetAt(endX, endTop) }
    })()
  `)
}

function readClipboard(viewer: TauriPage): Promise<string> {
  return viewer.evaluate<string>(
    `(async () => { try { return await navigator.clipboard.readText() } catch { return '' } })()`,
  )
}

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
  await expect.poll(() => wrapBadgeVisible(viewer), { timeout: waitBudget(3000) }).toBe(on)
}

/** Turns wrap back off from a `finally`, so the persisted setting returns to its
 *  default for later specs even when an assertion above threw. ❗ A failure here
 *  once left wrap on, and the drag-copy specs in `viewer.spec.ts` then ran on
 *  wrapped rows they weren't written for (that leak surfaced #962). A cleanup error
 *  must not hide the test's own failure, so it only logs. */
async function restoreWordWrapOff(viewer: TauriPage): Promise<void> {
  try {
    await setWordWrap(viewer, false)
  } catch (err) {
    console.warn('Could not turn word wrap back off:', err)
  }
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
      await expect
        .poll(() => scrollToBottomAndCheckLastLineVisible(viewer), { timeout: waitBudget(10000) })
        .toBe('visible')

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
    } finally {
      await restoreWordWrapOff(viewer)
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
      await expect
        .poll(() => scrollToFractionAndCheckFilled(viewer, 0.5), { timeout: waitBudget(10000) })
        .toBe('filled')

      // And the end of the file stays reachable (a wrong total height puts the
      // last line off-screen). Sample across the ready flip.
      await expect
        .poll(() => scrollToBottomAndCheckLastLineVisible(viewer, binaryLastLineSelector), {
          timeout: waitBudget(10000),
        })
        .toBe('visible')
      for (let i = 0; i < 5; i++) {
        // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- sampling interval of a stability assertion, not a readiness wait
        await sleep(100)
        expect(await scrollToBottomAndCheckLastLineVisible(viewer, binaryLastLineSelector)).toBe('visible')
      }
    } finally {
      await restoreWordWrapOff(viewer)
      await closeScopedWindow(mainPage, viewer, label)
      fs.rmSync(binaryFilePath, { force: true })
    }
  })

  test('with wrap on, a drag across a wrapped no-space row copies the dragged slice', async ({ tauriPage }) => {
    const mainPage = tauriPage as TauriPage
    fs.writeFileSync(noSpaceFilePath, `${noSpaceLine}\n`)

    const viewer = await openViewerWindow(mainPage, noSpaceFilePath)
    const label = viewer.targetWindow
    if (!label) throw new Error('Scoped viewer page has no targetWindow label')

    try {
      await viewer.waitForSelector('.viewer-container[data-window-ready="loaded"]', 10000)
      await setWordWrap(viewer, true)

      for (const mode of ['across-wrap', 'in-leading'] as const) {
        const drag = await dragOnWrappedRow(viewer, mode)
        expect(drag.visualRows).toBeGreaterThan(1)
        const expected = noSpaceLine.slice(drag.from, drag.to)
        expect(expected.length, `${mode}: the drag spans characters`).toBeGreaterThan(0)

        await viewer.evaluate(
          `window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', ${PRIMARY_KEY_FLAG}: true }))`,
        )
        // Poll the clipboard itself: the first copy's toast may still be up for the second.
        await expect
          .poll(() => readClipboard(viewer), {
            timeout: waitBudget(5000),
            message: `${mode} copy (leading ${String(drag.leadingPx)} px)`,
          })
          .toBe(expected)
      }
    } finally {
      await restoreWordWrapOff(viewer)
      await closeScopedWindow(mainPage, viewer, label)
      fs.rmSync(noSpaceFilePath, { force: true })
    }
  })
})
