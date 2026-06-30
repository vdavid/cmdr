import { describe, it, expect } from 'vitest'
import { computeOverlapShift, type ShiftRect } from './select-positioning'

// A roomy viewport so nothing clamps unless a test wants it to.
const VIEWPORT = { viewportWidth: 1440, viewportHeight: 900, pad: 8 }

function rect(partial: Partial<ShiftRect>): ShiftRect {
  return { left: 0, right: 0, top: 0, bottom: 0, height: 0, ...partial }
}

describe('computeOverlapShift', () => {
  it('lands the checked row on the trigger when the menu opened below it (no shift yet)', () => {
    // Trigger value at y≈100; menu opened below with the checked row's label at y≈200.
    const shift = computeOverlapShift({
      trigger: rect({ left: 200, right: 260, top: 100, bottom: 116, height: 16 }),
      item: rect({ left: 224, right: 280, top: 200, bottom: 216, height: 16 }),
      content: rect({ left: 200, right: 380, top: 196, bottom: 296 }),
      shiftX: 0,
      shiftY: 0,
      ...VIEWPORT,
    })
    // Horizontal: align the label's left to the trigger value's left → 200 - 224 = -24.
    expect(shift.x).toBe(-24)
    // Vertical: align centers → (100+8) - (200+8) = -100.
    expect(shift.y).toBe(-100)
  })

  it('is self-correcting: folds the residual gap into an already-applied shift', () => {
    // Same geometry but the content already carries a shift, so the rects reflect it. The result
    // must be the same absolute landing spot as the unshifted case.
    const shift = computeOverlapShift({
      trigger: rect({ left: 200, right: 260, top: 100, bottom: 116, height: 16 }),
      item: rect({ left: 214, right: 270, top: 150, bottom: 166, height: 16 }),
      content: rect({ left: 190, right: 370, top: 146, bottom: 246 }),
      shiftX: -10,
      shiftY: -50,
      ...VIEWPORT,
    })
    expect(shift.x).toBe(-24)
    expect(shift.y).toBe(-100)
  })

  it('clamps so the content never goes above the top viewport edge', () => {
    // A big upward shift would push the content off the top; clamp pins it to `pad`.
    const shift = computeOverlapShift({
      trigger: rect({ left: 200, right: 260, top: 20, bottom: 36, height: 16 }),
      item: rect({ left: 200, right: 280, top: 500, bottom: 516, height: 16 }),
      content: rect({ left: 200, right: 380, top: 30, bottom: 430 }),
      shiftX: 0,
      shiftY: 0,
      ...VIEWPORT,
    })
    // Desired dy ≈ (20+8) - (500+8) = -480, which would put content top at 30 - 480 = -450.
    // Clamp to pad (8): dy = 8 - 30 = -22 → content top lands at 8.
    expect(shift.y).toBe(-22)
  })

  it('clamps so the content never goes below the bottom viewport edge', () => {
    const shift = computeOverlapShift({
      trigger: rect({ left: 200, right: 260, top: 850, bottom: 866, height: 16 }),
      item: rect({ left: 200, right: 280, top: 100, bottom: 116, height: 16 }),
      content: rect({ left: 200, right: 380, top: 96, bottom: 860 }),
      shiftX: 0,
      shiftY: 0,
      ...VIEWPORT,
    })
    // Bottom edge allowed: 900 - 8 = 892; base bottom 860 → max dy = 32.
    expect(shift.y).toBe(32)
  })

  it('clamps horizontally so a wide menu stays within the right edge', () => {
    const shift = computeOverlapShift({
      // Aligning the label to the trigger value wants a rightward (positive) shift...
      trigger: rect({ left: 1410, right: 1462, top: 100, bottom: 116, height: 16 }),
      item: rect({ left: 1404, right: 1460, top: 100, bottom: 116, height: 16 }),
      content: rect({ left: 1380, right: 1432, top: 116, bottom: 216 }),
      shiftX: 0,
      shiftY: 0,
      ...VIEWPORT,
    })
    // ...but the right edge allowed is 1440 - 8 = 1432 and base right is already 1432, so max
    // dx = 0: it can't move right.
    expect(shift.x).toBe(0)
  })

  it('pins to the top edge when the content is taller than the viewport', () => {
    const shift = computeOverlapShift({
      trigger: rect({ left: 200, right: 260, top: 400, bottom: 416, height: 16 }),
      item: rect({ left: 200, right: 280, top: 400, bottom: 416, height: 16 }),
      content: rect({ left: 200, right: 380, top: 10, bottom: 1200 }),
      shiftX: 0,
      shiftY: 0,
      ...VIEWPORT,
    })
    // min (pad - baseTop = -2) > max (negative) → pinned to min.
    expect(shift.y).toBe(-2)
  })
})
