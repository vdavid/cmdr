/**
 * Unit tests for `createTypeToJumpState`.
 *
 * Drives the factory through its public surface (no peeking at timers). The
 * key invariants tested here are:
 *
 * - The buffer accumulates lowercased chars, the indicator turns visible.
 * - The 1 s buffer-reset timer clears the buffer but keeps the indicator
 *   visible in the "stale" state.
 * - The 5 s indicator-hide timer hides everything.
 * - Each `appendChar` bumps `generation`, so callers can discard out-of-order
 *   IPC responses.
 * - `getResetMs` is read on each keystroke (so a live setting change applies
 *   on the next keypress, not later).
 *
 * Uses Vitest fake timers — see `smb-reconnect-manager.svelte.test.ts` for the
 * same pattern.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createTypeToJumpState, INDICATOR_HIDE_MS } from './type-to-jump-state.svelte'

describe('createTypeToJumpState', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('starts with an empty buffer and a hidden indicator', () => {
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn() })
    expect(state.buffer).toBe('')
    expect(state.indicatorVisible).toBe(false)
    expect(state.indicatorStale).toBe(false)
    expect(state.generation).toBe(0)
  })

  it('appendChar lowercases the input and shows the indicator', () => {
    const onMatch = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch })
    state.appendChar('T')
    expect(state.buffer).toBe('t')
    expect(state.indicatorVisible).toBe(true)
    expect(state.indicatorStale).toBe(false)
    expect(onMatch).toHaveBeenCalledWith('t', 1)
  })

  it('appendChar calls accumulate into the buffer', () => {
    const onMatch = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch })
    state.appendChar('T')
    state.appendChar('e')
    state.appendChar('S')
    expect(state.buffer).toBe('tes')
    expect(state.generation).toBe(3)
    expect(onMatch).toHaveBeenLastCalledWith('tes', 3)
  })

  it('clear() empties the buffer, hides the indicator, and stops the timers', () => {
    const onIndicatorHide = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn(), onIndicatorHide })
    state.appendChar('a')
    expect(state.indicatorVisible).toBe(true)

    state.clear()
    expect(state.buffer).toBe('')
    expect(state.indicatorVisible).toBe(false)
    expect(state.indicatorStale).toBe(false)

    // Timers must be cancelled — advancing past the hide delay must not fire
    // the hide callback nor flip any state.
    vi.advanceTimersByTime(INDICATOR_HIDE_MS + 100)
    expect(onIndicatorHide).not.toHaveBeenCalled()
    expect(state.buffer).toBe('')
    expect(state.indicatorVisible).toBe(false)
  })

  it('after the reset delay, the buffer empties but the indicator stays visible (stale)', () => {
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn() })
    state.appendChar('a')
    state.appendChar('b')
    expect(state.buffer).toBe('ab')

    vi.advanceTimersByTime(1000)
    expect(state.buffer).toBe('')
    expect(state.indicatorVisible).toBe(true)
    expect(state.indicatorStale).toBe(true)
  })

  it('after the indicator-hide delay, the indicator is hidden', () => {
    const onIndicatorHide = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn(), onIndicatorHide })
    state.appendChar('a')

    vi.advanceTimersByTime(INDICATOR_HIDE_MS)
    expect(state.indicatorVisible).toBe(false)
    expect(state.indicatorStale).toBe(false)
    expect(state.buffer).toBe('')
    expect(onIndicatorHide).toHaveBeenCalledTimes(1)
  })

  it('typing after a stale reset starts a fresh buffer', () => {
    const onMatch = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch })
    state.appendChar('c')
    state.appendChar('o')
    expect(state.buffer).toBe('co')

    // Buffer-reset fires; indicator is now stale.
    vi.advanceTimersByTime(1000)
    expect(state.indicatorStale).toBe(true)
    expect(state.buffer).toBe('')

    // Next keystroke clears the stale flag and starts a fresh buffer — must
    // not append to a previous "co".
    state.appendChar('s')
    expect(state.buffer).toBe('s')
    expect(state.indicatorStale).toBe(false)
    expect(state.indicatorVisible).toBe(true)
  })

  it('generation increments per keystroke and supports race-protection checks', () => {
    const captured: Array<{ buffer: string; generation: number }> = []
    const onMatch = (buffer: string, generation: number) => {
      captured.push({ buffer, generation })
    }
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch })

    state.appendChar('a')
    state.appendChar('b')
    state.appendChar('c')

    expect(captured).toEqual([
      { buffer: 'a', generation: 1 },
      { buffer: 'ab', generation: 2 },
      { buffer: 'abc', generation: 3 },
    ])

    // Simulate an out-of-order IPC response for keystroke 2 arriving after 3.
    // The caller's guard is `generation !== state.generation` — verify the
    // contract by checking against the live counter.
    const staleGen = captured[1].generation
    expect(staleGen).toBe(2)
    expect(staleGen === state.generation).toBe(false) // discard
    const freshGen = captured[2].generation
    expect(freshGen === state.generation).toBe(true) // apply
  })

  it('reads getResetMs() on every keystroke so a live setting change applies on the next press', () => {
    let resetMs = 1000
    const state = createTypeToJumpState({ getResetMs: () => resetMs, onMatch: vi.fn() })

    state.appendChar('a')
    // Change the setting between keystrokes — the next press should schedule
    // a timer using the NEW value.
    resetMs = 500
    state.appendChar('b')

    // 400 ms in: nothing yet (the new timer hasn't fired).
    vi.advanceTimersByTime(400)
    expect(state.buffer).toBe('ab')
    expect(state.indicatorStale).toBe(false)

    // Cross the 500 ms boundary — the timer scheduled with resetMs=500 fires.
    vi.advanceTimersByTime(101)
    expect(state.buffer).toBe('')
    expect(state.indicatorStale).toBe(true)
  })

  it('keystrokes restart both timers — slow continuous typing never hides the indicator', () => {
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn() })
    state.appendChar('a')

    // Press another key every 800 ms (less than reset delay) — the buffer
    // must keep growing and never go stale.
    for (let i = 0; i < 10; i++) {
      vi.advanceTimersByTime(800)
      state.appendChar('x')
    }
    expect(state.buffer.length).toBe(11)
    expect(state.indicatorStale).toBe(false)
    expect(state.indicatorVisible).toBe(true)
  })

  it('calls the optional logger if provided', () => {
    const log = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn(), log })
    state.appendChar('a')

    vi.advanceTimersByTime(1000)
    expect(log).toHaveBeenCalledWith('type-to-jump: buffer reset (stale)')

    vi.advanceTimersByTime(INDICATOR_HIDE_MS - 1000)
    expect(log).toHaveBeenCalledWith('type-to-jump: indicator hidden')
  })

  it('clear() is idempotent — multiple calls do not throw or fire callbacks', () => {
    const onIndicatorHide = vi.fn()
    const state = createTypeToJumpState({ getResetMs: () => 1000, onMatch: vi.fn(), onIndicatorHide })
    state.clear()
    state.clear()
    state.clear()
    expect(state.buffer).toBe('')
    expect(state.indicatorVisible).toBe(false)
    expect(onIndicatorHide).not.toHaveBeenCalled()
  })
})
