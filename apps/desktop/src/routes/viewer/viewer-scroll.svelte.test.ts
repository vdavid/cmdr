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
