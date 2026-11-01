/**
 * Waiting for what lands a frame late (`afterNextFrame`: the deferred active descendant, the live
 * announcer) without betting on how long a frame takes. A fixed sleep past the 100 ms frame
 * fallback reads too early when a stall eats its margin, and an assertion that something is ABSENT
 * then passes without checking anything.
 */

import { expect, vi } from 'vitest'
import { afterNextFrame } from '#lib/ui/deferred-active-descendant.svelte.js'

/** A backstop for a trip meaning a regression, under vitest's 5 s per-test timeout so the assertion's own message wins. */
const BACKSTOP_MS = 4000

/** Retries `assertion` until it holds, and fails with its last error if it never does. */
export async function eventually(assertion: () => void | Promise<void>): Promise<void> {
  await vi.waitFor(assertion, { timeout: BACKSTOP_MS, interval: 5 })
}

/** Waits for the focused element's held-back `aria-activedescendant` to land. */
export async function untilFocusNamesARow(): Promise<void> {
  await eventually(() => {
    expect(document.activeElement?.getAttribute('aria-activedescendant')).toBeTruthy()
  })
}

/** One trip through the event loop, which drains every microtask: Svelte's flush and the observers it wakes. */
async function drainMicrotasks(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0))
}

/**
 * Resolves once every `afterNextFrame` already asked for has run and what it changed has flushed,
 * for checking that something did NOT land. Scheduling our own after theirs is what makes this
 * exact: frames run their callbacks in order, and an earlier fallback timer fires first.
 */
export async function pastPendingFrames(): Promise<void> {
  await drainMicrotasks()
  await new Promise<void>((resolve) => {
    afterNextFrame(resolve)
  })
  await drainMicrotasks()
}
