/**
 * The one ETA smoother, shared by the copy dialog and the operation queue window.
 *
 * The regression this pins: the dialog smoothed the backend's ETA while the
 * queue row rendered it raw, so one operation showed "8m 12s remaining" in one
 * window and "5m 46s" in the other at the same moment.
 */
import { describe, it, expect } from 'vitest'
import { createEtaSmoother, ETA_SMOOTHING_FACTOR } from './progress-readout'
import { seconds } from '$lib/units'

describe('createEtaSmoother', () => {
  it('adopts the first value as-is, with no warm-up lag', () => {
    const smoother = createEtaSmoother()
    expect(smoother.push(seconds(12))).toBe(12)
  })

  it('moves a fraction of the gap per tick, damping single-tick jitter', () => {
    const smoother = createEtaSmoother()
    smoother.push(seconds(10))
    // 10 + 0.25 * (20 - 10) = 12.5
    expect(smoother.push(seconds(20))).toBeCloseTo(10 + ETA_SMOOTHING_FACTOR * 10)
  })

  it('closes most of a sustained change within about a second of ticks', () => {
    // Progress ticks land every ~200 ms, so five ticks is roughly a second: a
    // real slowdown has to show up by then, not creep in over a minute.
    const smoother = createEtaSmoother()
    smoother.push(seconds(100))
    for (let i = 0; i < 5; i++) smoother.push(seconds(50))
    expect(smoother.value).toBeLessThan(50 + 0.25 * 50)
    expect(smoother.value).toBeGreaterThan(50)
  })

  it('clears on a null, so a phase change re-warms instead of dragging a stale value', () => {
    const smoother = createEtaSmoother()
    smoother.push(seconds(30))
    expect(smoother.push(null)).toBeNull()
    expect(smoother.value).toBeNull()
    expect(smoother.push(seconds(90))).toBe(90)
  })

  it('reset() drops the history without needing a null tick', () => {
    const smoother = createEtaSmoother()
    smoother.push(seconds(30))
    smoother.reset()
    expect(smoother.value).toBeNull()
    expect(smoother.push(seconds(90))).toBe(90)
  })

  it('gives both windows the same number for the same tick sequence', () => {
    // Two independent smoothers fed the same backend stream must agree — that's
    // the whole contract. Anything stateful and per-window has to be shared, not
    // reimplemented.
    const dialog = createEtaSmoother()
    const queueRow = createEtaSmoother()
    for (const raw of [480, 470, 500, 460, 455, 450]) {
      expect(dialog.push(seconds(raw))).toBe(queueRow.push(seconds(raw)))
    }
  })
})
