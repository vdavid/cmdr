import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createTouchHold, isTouchPress, TOUCH_HOLD_MS, TOUCH_HOLD_SLOP_PX } from './touch-hold'

function pointer(type: string, init: PointerEventInit = {}): PointerEvent {
  return new PointerEvent(type, { pointerId: 7, pointerType: 'touch', isPrimary: true, button: 0, ...init })
}

function touchMove(): TouchEvent {
  return new TouchEvent('touchmove', { cancelable: true })
}

describe('createTouchHold', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    // Lift any finger a test left down, so a hold never leaks into the next one.
    document.dispatchEvent(pointer('pointerup'))
    vi.useRealTimers()
  })

  it('arms once a touch stays still long enough', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown', { clientX: 10, clientY: 10 }), onHold)
    vi.advanceTimersByTime(TOUCH_HOLD_MS - 1)
    expect(onHold).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(onHold).toHaveBeenCalledOnce()
  })

  it('treats a touch that moves before the hold as a scroll', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown', { clientX: 10, clientY: 10 }), onHold)
    document.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 11 + TOUCH_HOLD_SLOP_PX }))
    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    expect(onHold).not.toHaveBeenCalled()
  })

  it('keeps a touch that wanders inside the slop', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown', { clientX: 10, clientY: 10 }), onHold)
    document.dispatchEvent(pointer('pointermove', { clientX: 14, clientY: 13 }))
    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    expect(onHold).toHaveBeenCalledOnce()
  })

  it('never arms a tap', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown'), onHold)
    document.dispatchEvent(pointer('pointerup'))
    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    expect(onHold).not.toHaveBeenCalled()
  })

  it('lets the OS long press have the press when its menu comes first', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown'), onHold)
    document.dispatchEvent(new MouseEvent('contextmenu'))
    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    expect(onHold).not.toHaveBeenCalled()
  })

  it('ignores a mouse, which arms on its own mousedown', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown', { pointerType: 'mouse' }), onHold)
    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    expect(onHold).not.toHaveBeenCalled()
  })

  it('ignores another finger moving', () => {
    const onHold = vi.fn()
    createTouchHold().press(pointer('pointerdown', { clientX: 10, clientY: 10 }), onHold)
    document.dispatchEvent(pointer('pointermove', { pointerId: 8, clientX: 200, clientY: 200 }))
    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    expect(onHold).toHaveBeenCalledOnce()
  })

  it('stops the page from scrolling while held, until that finger lifts', () => {
    createTouchHold().press(pointer('pointerdown'), vi.fn())

    const beforeHold = touchMove()
    window.dispatchEvent(beforeHold)
    expect(beforeHold.defaultPrevented).toBe(false)

    vi.advanceTimersByTime(TOUCH_HOLD_MS)
    const held = touchMove()
    window.dispatchEvent(held)
    expect(held.defaultPrevented).toBe(true)

    document.dispatchEvent(pointer('pointerup'))
    const afterLift = touchMove()
    window.dispatchEvent(afterLift)
    expect(afterLift.defaultPrevented).toBe(false)
  })
})

describe('isTouchPress', () => {
  it('names a finger and a pen, never a mouse or a plain mousedown', () => {
    expect(isTouchPress(pointer('pointerdown'))).toBe(true)
    expect(isTouchPress(pointer('pointerdown', { pointerType: 'pen' }))).toBe(true)
    expect(isTouchPress(pointer('pointerdown', { pointerType: 'mouse' }))).toBe(false)
    expect(isTouchPress(new MouseEvent('mousedown'))).toBe(false)
  })
})
