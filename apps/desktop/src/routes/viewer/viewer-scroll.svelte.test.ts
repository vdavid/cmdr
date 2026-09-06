/**
 * Regression test for the fraction-seek divisor in `createViewerScroll.fetchLines`.
 *
 * When the backend can't seek by line (`getTotalLines() === null`) the seek is sent as a
 * fraction (`fetchFrom / estimatedTotalLines()`). If the estimate is 0, that division
 * yields `NaN` (0/0) or `Infinity` (>0/0); both serialize to JSON `null` over IPC and the
 * Rust `viewer_get_lines` command rejects the `f64 targetValue` ("invalid type: null,
 * expected f64"). This crashed the line fetch in production (ERR-9XYEF, ERR-6JYVE).
 *
 * The contract: the value sent to the backend must always be a finite number.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

import { createViewerScroll } from './viewer-scroll.svelte'
import { EOF_LINE } from './selection.svelte'
import type { LineChunk } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

const chunk: LineChunk = {
  lines: ['x'],
  firstLineNumber: 0,
  byteOffset: 0,
  totalLines: null,
  totalBytes: 1000,
}

describe('createViewerScroll fraction seek', () => {
  it('sends a finite targetValue when the line-count estimate is 0', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_get_lines', () => chunk)

    // A byte-seek backend that doesn't know its total lines, with a 0 estimate.
    const scroll = createViewerScroll({
      getSessionId: () => 'sess-1',
      getTotalLines: () => null,
      setTotalLines: () => {},
      getEstimatedLines: () => 0,
      getBackendType: () => 'byteSeek',
      onTimeoutError: () => {},
      getAllLines: () => null,
      getTextWidth: () => 0,
    })

    scroll.fetchVisibleNow()
    await vi.waitFor(() => {
      expect(ipc.lastCall('viewer_get_lines')).toBeDefined()
    })

    const call = ipc.lastCall('viewer_get_lines')
    expect(call?.payload).toMatchObject({ targetType: 'fraction' })
    const targetValue = (call?.payload as { targetValue: number }).targetValue
    expect(Number.isFinite(targetValue)).toBe(true)
  })
})

describe('createViewerScroll.ensureLineVisible', () => {
  /** A scroll composable wired to a fake scroller of `scrollHeight` in a `clientHeight` box. */
  function wireWithScroller(scrollHeight: number, clientHeight: number) {
    installIpcMock().mock('viewer_get_lines', () => chunk)
    const scroll = createViewerScroll({
      getSessionId: () => 'sess-1',
      // No line index yet, which is exactly when `⌘⇧Down` mints the sentinel.
      getTotalLines: () => null,
      setTotalLines: () => {},
      getEstimatedLines: () => 1000,
      getBackendType: () => 'byteSeek',
      onTimeoutError: () => {},
      getAllLines: () => null,
      getTextWidth: () => 0,
    })
    const el = document.createElement('div')
    // jsdom lays nothing out, so the two geometry reads have to be supplied.
    Object.defineProperty(el, 'scrollHeight', { value: scrollHeight })
    Object.defineProperty(el, 'clientHeight', { value: clientHeight })
    scroll.contentRef = el
    return { scroll, el }
  }

  it('reads the end-of-file sentinel as the end of the file', () => {
    const { scroll, el } = wireWithScroller(50_000, 800)
    el.scrollTop = 0

    scroll.ensureLineVisible(EOF_LINE)

    // Not line arithmetic on `Number.MAX_SAFE_INTEGER`: the bottom of the scroller.
    expect(el.scrollTop).toBe(49_200)
  })

  it('never lands on NaN, which would throw the view to the top of the file', () => {
    const { scroll, el } = wireWithScroller(50_000, 800)
    el.scrollTop = 1234

    scroll.ensureLineVisible(EOF_LINE)

    expect(Number.isNaN(el.scrollTop)).toBe(false)
    expect(el.scrollTop).toBeGreaterThan(0)
  })
})

describe('createViewerScroll.renderedLineText', () => {
  /** A composable over a `totalLines`-line file, unscrolled, at the default 600px viewport. */
  function wire(totalLines: number) {
    return createViewerScroll({
      getSessionId: () => 'sess-1',
      getTotalLines: () => totalLines,
      setTotalLines: () => {},
      getEstimatedLines: () => totalLines,
      getBackendType: () => 'lineIndex',
      onTimeoutError: () => {},
      getAllLines: () => null,
      getTextWidth: () => 0,
    })
  }

  it('hands back the cached text of a rendered line', () => {
    const scroll = wire(11)
    scroll.lineCache.set(3, 'hello')

    expect(scroll.renderedLineText(3)).toBe('hello')
  })

  it("reads a rendered line the cache doesn't hold as the empty row the template draws for it", () => {
    // A trailing newline makes the `lineIndex` backend count a last line it never emits:
    // `totalLines` 11 for real lines 0-9. The template still draws a row for line 10.
    const scroll = wire(11)
    for (let i = 0; i < 10; i++) scroll.lineCache.set(i, 'line')

    expect(scroll.renderedLineText(10)).toBe('')
  })

  it('stays undefined outside the rendered range, so the caller knows to scroll and retry', () => {
    // 600px of viewport plus the buffer reaches line ~84 of 40 001, nowhere near the end.
    const scroll = wire(40_001)

    expect(scroll.renderedLineText(40_000)).toBeUndefined()
  })
})
