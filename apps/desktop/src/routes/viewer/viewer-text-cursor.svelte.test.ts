/**
 * Unit tests for the text cursor's coordinate conversion.
 *
 * `caretRectFor`'s own edge rule is pinned in `viewer-pointer.test.ts`, which owns the
 * fake-layout harness; this file covers the one step on top of it.
 */

import { describe, it, expect } from 'vitest'

import { toSpacerRelative } from './viewer-text-cursor.svelte'
import type { CaretRect } from './viewer-caret-geometry'

function caret(left: number, top: number, bottom: number): CaretRect {
  return { left, right: left, top, bottom }
}

describe('toSpacerRelative', () => {
  it('subtracts the spacer origin from the measured viewport rect', () => {
    const box = toSpacerRelative(caret(120, 50, 68), { left: 10, top: 20 })
    expect(box).toEqual({ left: 110, top: 30, height: 18 })
  })

  it('lands on the scrolled-away part of the spacer, far below its own origin', () => {
    // The realistic case: `.scroll-spacer` is metres tall and its top has scrolled far
    // above the viewport, so its `top` is a large negative number while the measured
    // caret sits at a small positive one. Pre-fix, applying `linesOffset` on top of this
    // put the cursor 10⁵-10⁷ px off screen.
    const box = toSpacerRelative(caret(48, 300, 318), { left: 0, top: -120_000 })
    expect(box).toEqual({ left: 48, top: 120_300, height: 18 })
  })

  it('keeps sub-pixel geometry rather than rounding it to the nearest device pixel', () => {
    const box = toSpacerRelative(caret(120.5, 50.25, 68.75), { left: 0.5, top: 0.25 })
    expect(box).toEqual({ left: 120, top: 50, height: 18.5 })
  })

  it('refuses a zero-height rect: an unlaid-out row has nothing to paint', () => {
    expect(toSpacerRelative(caret(120, 50, 50), { left: 0, top: 0 })).toBeNull()
  })

  it('refuses an inverted rect rather than painting a negative-height bar', () => {
    expect(toSpacerRelative(caret(120, 68, 50), { left: 0, top: 0 })).toBeNull()
  })
})
