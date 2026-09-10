/**
 * Tests for `createViewerPull`: when the viewer shows its "fetching this file" bar
 * (only once an open has pulled for a second, and only with real progress), what
 * the bar draws (a fraction when the size is known, bytes alone when it isn't), when
 * it reads as halted, and that a finished open ignores a late report.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { createViewerPull, PULL_BAR_DELAY_MS, PULL_STALLED_AFTER_MS } from './viewer-pull.svelte'

describe('createViewerPull', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('stays hidden for the first second, even with progress', () => {
    const pull = createViewerPull()
    pull.start()
    pull.report({ bytesDone: 1024, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS - 1)
    expect(pull.visible).toBe(false)
    pull.destroy()
  })

  it('shows after a second once a pull has reported progress', () => {
    const pull = createViewerPull()
    pull.start()
    pull.report({ bytesDone: 1024, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    expect(pull.visible).toBe(true)
    expect(pull.bytesDone).toBe(1024)
    expect(pull.bytesTotal).toBe(4096)
    expect(pull.fraction).toBe(0.25)
    pull.destroy()
  })

  it('stays hidden past a second when nothing is being pulled (a plain slow open)', () => {
    const pull = createViewerPull()
    pull.start()
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS * 3)
    expect(pull.visible).toBe(false)
    pull.destroy()
  })

  it('has no fraction when the source did not say how big the file is', () => {
    const pull = createViewerPull()
    pull.start()
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    pull.report({ bytesDone: 2048, bytesTotal: null })
    expect(pull.visible).toBe(true)
    expect(pull.fraction).toBeNull()
    expect(pull.bytesDone).toBe(2048)
    pull.destroy()
  })

  it('reads as stalled once no new bytes arrive for a while, and moving again on news', () => {
    const pull = createViewerPull()
    pull.start()
    pull.report({ bytesDone: 1024, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    expect(pull.stalled).toBe(false)

    // The same count again is not news.
    pull.report({ bytesDone: 1024, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_STALLED_AFTER_MS)
    expect(pull.stalled).toBe(true)

    pull.report({ bytesDone: 2048, bytesTotal: 4096 })
    expect(pull.stalled).toBe(false)
    pull.destroy()
  })

  it('hides on finish and ignores a report that arrives after it', () => {
    const pull = createViewerPull()
    pull.start()
    pull.report({ bytesDone: 1024, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    pull.finish()
    expect(pull.visible).toBe(false)

    pull.report({ bytesDone: 4096, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    expect(pull.visible).toBe(false)
    pull.destroy()
  })

  it('starts each open from scratch, so a retry never shows the last attempt', () => {
    const pull = createViewerPull()
    pull.start()
    pull.report({ bytesDone: 3000, bytesTotal: 4096 })
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    pull.finish()

    pull.start()
    expect(pull.bytesDone).toBe(0)
    vi.advanceTimersByTime(PULL_BAR_DELAY_MS)
    expect(pull.visible).toBe(false)
    pull.destroy()
  })
})
