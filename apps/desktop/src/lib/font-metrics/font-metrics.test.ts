// Orchestration tests: who measures, how often, and what happens on failure.
//
// These pin the behaviour behind a real freeze: a text-size change had every
// window measure the same font at once, on a thread they share, with no
// in-flight guard to collapse the duplicate passes.

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  ensureFontMetricsLoaded,
  fillMissingFontMetrics,
  getCurrentFontId,
  setMeasuresFontMetrics,
  resetFontMetricsStateForTests,
} from './index'
import type { FontSpec } from './measure'
import type { MeasureResult } from './worker-client'

/** `storeFontMetrics` and `extendFontMetrics`'s real signatures (`file-listing.ts`) are
 *  positional all the way down to the raw IPC `invoke`, so the exposed mocks below (in the
 *  `$lib/tauri-commands` factory) stay positional too; these inner mocks are what tests
 *  actually assert against, as a named payload so a future edit can't silently swap
 *  `codePoints` and `widths`. */
interface FontMetricsWritePayload {
  fontId: string
  codePoints: number[]
  widths: number[]
}

const hasFontMetrics = vi.fn<(fontId: string) => Promise<boolean>>()
const storeFontMetrics = vi.fn<(payload: FontMetricsWritePayload) => Promise<void>>()
const extendFontMetrics = vi.fn<(payload: FontMetricsWritePayload) => Promise<void>>()
const measureOffMainThread = vi.fn<(spec: FontSpec, codePoints: Uint32Array) => Promise<MeasureResult>>()

vi.mock('$lib/tauri-commands', () => ({
  hasFontMetrics: (fontId: string) => hasFontMetrics(fontId),
  storeFontMetrics: (fontId: string, codePoints: number[], widths: number[]) =>
    storeFontMetrics({ fontId, codePoints, widths }),
  extendFontMetrics: (fontId: string, codePoints: number[], widths: number[]) =>
    extendFontMetrics({ fontId, codePoints, widths }),
}))

vi.mock('./worker-client', () => ({
  measureOffMainThread: (spec: FontSpec, codePoints: Uint32Array) => measureOffMainThread(spec, codePoints),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

/** Resolves only when `release()` is called, so overlap is observable. */
function deferredMeasurement() {
  let release = () => {}
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  measureOffMainThread.mockImplementation(async (_spec, codePoints) => {
    await gate
    return { widths: new Float32Array(codePoints.length).fill(7), via: 'worker' as const }
  })
  return {
    release: () => {
      release()
    },
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  resetFontMetricsStateForTests()
  hasFontMetrics.mockResolvedValue(false)
  storeFontMetrics.mockResolvedValue(undefined)
  extendFontMetrics.mockResolvedValue(undefined)
  measureOffMainThread.mockImplementation((_spec, codePoints) =>
    Promise.resolve({ widths: new Float32Array(codePoints.length).fill(7), via: 'worker' as const }),
  )
})

describe('the measuring window gate', () => {
  it('measures nothing in a window that did not opt in', async () => {
    // The transfer-queue, settings, viewer and shortcuts windows all run
    // `initTextSize` but never render Brief mode.
    await ensureFontMetricsLoaded()

    expect(measureOffMainThread).not.toHaveBeenCalled()
    expect(storeFontMetrics).not.toHaveBeenCalled()
  })

  it('measures in the window that opted in', async () => {
    setMeasuresFontMetrics(true)

    await ensureFontMetricsLoaded()

    expect(measureOffMainThread).toHaveBeenCalledTimes(1)
    expect(storeFontMetrics).toHaveBeenCalledTimes(1)
  })

  it('fills nothing in a window that did not opt in', async () => {
    expect(await fillMissingFontMetrics(getCurrentFontId(), [0x4e00])).toBe(false)
    expect(measureOffMainThread).not.toHaveBeenCalled()
  })
})

describe('ensureFontMetricsLoaded', () => {
  beforeEach(() => {
    setMeasuresFontMetrics(true)
  })

  it('collapses concurrent callers into one measurement', async () => {
    // DualPaneExplorer's mount, the text-size debounce, and BriefList's retry
    // can all land here at once.
    const { release } = deferredMeasurement()

    const all = Promise.all([ensureFontMetricsLoaded(), ensureFontMetricsLoaded(), ensureFontMetricsLoaded()])
    release()
    await all

    expect(measureOffMainThread).toHaveBeenCalledTimes(1)
    expect(storeFontMetrics).toHaveBeenCalledTimes(1)
  })

  it('skips measuring entirely when Rust already has the font', async () => {
    hasFontMetrics.mockResolvedValue(true)

    await ensureFontMetricsLoaded()

    expect(measureOffMainThread).not.toHaveBeenCalled()
  })

  it('sends code points and widths as equal-length parallel arrays', async () => {
    await ensureFontMetricsLoaded()

    const [{ fontId, codePoints, widths }] = storeFontMetrics.mock.calls[0]
    expect(fontId).toBe(getCurrentFontId())
    expect(codePoints.length).toBe(widths.length)
    expect(codePoints.length).toBeGreaterThan(0)
  })

  it('re-measures after a failure instead of wedging on the in-flight entry', async () => {
    measureOffMainThread.mockRejectedValueOnce(new Error('worker died'))

    await ensureFontMetricsLoaded()
    expect(storeFontMetrics).not.toHaveBeenCalled()

    await ensureFontMetricsLoaded()
    expect(storeFontMetrics).toHaveBeenCalledTimes(1)
  })
})

describe('fillMissingFontMetrics', () => {
  const fontId = 'system-400-12'

  beforeEach(() => {
    setMeasuresFontMetrics(true)
  })

  it('measures the reported code points and merges them in', async () => {
    const filled = await fillMissingFontMetrics(fontId, [0x4e00, 0xac00])

    expect(filled).toBe(true)
    const [{ codePoints, widths }] = extendFontMetrics.mock.calls[0]
    expect(codePoints).toEqual([0x4e00, 0xac00])
    expect(widths.length).toBe(2)
  })

  it('measures a code point once however many listings report it', async () => {
    await fillMissingFontMetrics(fontId, [0x4e00])
    const second = await fillMissingFontMetrics(fontId, [0x4e00])

    expect(second).toBe(false)
    expect(extendFontMetrics).toHaveBeenCalledTimes(1)
  })

  it('measures only the code points it has not seen before', async () => {
    await fillMissingFontMetrics(fontId, [0x4e00])
    await fillMissingFontMetrics(fontId, [0x4e00, 0xac00])

    // The already-filled code point is not measured a second time.
    const [{ codePoints: secondBatch }] = extendFontMetrics.mock.calls[1]
    expect(secondBatch).toEqual([0xac00])
  })

  it('keeps font sizes independent', async () => {
    await fillMissingFontMetrics('system-400-12', [0x4e00])
    const otherSize = await fillMissingFontMetrics('system-400-15', [0x4e00])

    expect(otherSize).toBe(true)
    expect(extendFontMetrics).toHaveBeenCalledTimes(2)
  })

  it('drops unmeasurable code points rather than throwing on them', async () => {
    // A lone surrogate would make `String.fromCodePoint` throw and take the
    // whole batch down with it.
    const filled = await fillMissingFontMetrics(fontId, [0xd800, 0x4e00])

    expect(filled).toBe(true)
    const [{ codePoints }] = extendFontMetrics.mock.calls[0]
    expect(codePoints).toEqual([0x4e00])
  })

  it('reports nothing to do for an empty request', async () => {
    expect(await fillMissingFontMetrics(fontId, [])).toBe(false)
    expect(extendFontMetrics).not.toHaveBeenCalled()
  })

  it('re-arms after a failure so the gaps do not stay on the average forever', async () => {
    measureOffMainThread.mockRejectedValueOnce(new Error('worker died'))

    expect(await fillMissingFontMetrics(fontId, [0x4e00])).toBe(false)
    // The next report of the same code point must retry, not be deduplicated away.
    expect(await fillMissingFontMetrics(fontId, [0x4e00])).toBe(true)
    expect(extendFontMetrics).toHaveBeenCalledTimes(1)
  })
})
