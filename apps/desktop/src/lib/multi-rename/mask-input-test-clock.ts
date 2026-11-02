/**
 * The clock `MaskInput`'s tests run on. A block asserting the editor stays shut needs the rest
 * delays to have truly run, which a real sleep can't promise on a stalled machine: its timer and
 * the component's race, and a test that reads the field early passes without checking anything.
 */

import { tick } from 'svelte'
import { afterEach, beforeEach, vi } from 'vitest'
import { CARET_OPEN_MS, HOVER_CLOSE_MS, HOVER_OPEN_MS } from './token-editor-rules'

/** Lets rendering and promise chains finish, on either clock, without running any rest delay. */
export async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    if (vi.isFakeTimers()) await vi.advanceTimersByTimeAsync(1)
    else await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

/** Puts the enclosing `describe` on the fake clock. Its tests mount on it, so every timer is the clock's. */
export function useFakeClock(): void {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })
}

/** Runs past the caret's and the pointer's open and close delays, and whatever they set off. */
export async function pastEditorDelays(): Promise<void> {
  await vi.advanceTimersByTimeAsync(Math.max(CARET_OPEN_MS, HOVER_OPEN_MS, HOVER_CLOSE_MS))
  await settle()
}
