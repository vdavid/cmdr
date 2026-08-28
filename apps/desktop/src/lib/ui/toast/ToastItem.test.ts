/**
 * Component-level test for `ToastItem.svelte`.
 *
 * Covers the close-button tooltip, the `onUserDismiss` vs `onTimeout` split,
 * and the auto-dismiss rule: a transient toast hides at
 * `max(mountedAt + timeoutMs, pointerLeftAt + HOVER_LEAVE_GRACE_MS)`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import ToastItem from './ToastItem.svelte'
import { HOVER_LEAVE_GRACE_MS } from './toast-store.svelte'

function mountItem(props: Record<string, unknown>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ToastItem, {
    target,
    props: {
      id: 't1',
      content: 'Hello',
      level: 'info',
      dismissal: 'persistent',
      timeoutMs: 0,
      onTimeout: vi.fn(),
      onUserDismiss: vi.fn(),
      ...props,
    },
  })
  return target
}

describe('ToastItem close button', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows the configured tooltip on hover when closeTooltip is set', async () => {
    const target = mountItem({ closeTooltip: 'Close this notification' })
    await tick()

    const closeButton = target.querySelector('.toast-close')
    expect(closeButton).not.toBeNull()
    closeButton?.dispatchEvent(new MouseEvent('mouseenter'))

    // Tooltip action waits 400 ms before showing.
    vi.advanceTimersByTime(500)

    const tip = document.querySelector('.cmdr-tooltip')
    expect(tip?.textContent).toBe('Close this notification')
  })

  it('does not show a tooltip on hover when closeTooltip is unset', async () => {
    const target = mountItem({})
    await tick()

    const closeButton = target.querySelector('.toast-close')
    closeButton?.dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)

    const tip = document.querySelector('.cmdr-tooltip.visible')
    expect(tip).toBeNull()
  })

  it('calls onUserDismiss (not onTimeout) when X is clicked', async () => {
    const onUserDismiss = vi.fn()
    const onTimeout = vi.fn()
    const target = mountItem({ onUserDismiss, onTimeout, id: 'abc' })
    await tick()

    const closeButton = target.querySelector('.toast-close') as HTMLButtonElement
    closeButton.click()

    expect(onUserDismiss).toHaveBeenCalledWith('abc')
    expect(onTimeout).not.toHaveBeenCalled()
  })

  it('calls onTimeout (not onUserDismiss) when the auto-dismiss timer fires', async () => {
    const onUserDismiss = vi.fn()
    const onTimeout = vi.fn()
    mountItem({
      onUserDismiss,
      onTimeout,
      id: 'abc',
      dismissal: 'transient',
      timeoutMs: 1000,
    })
    await tick()

    vi.advanceTimersByTime(1500)

    expect(onTimeout).toHaveBeenCalledWith('abc')
    expect(onUserDismiss).not.toHaveBeenCalled()
  })
})

describe('ToastItem auto-dismiss rule', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('auto-dismisses a transient toast after timeoutMs (baseline)', async () => {
    const onTimeout = vi.fn()
    mountItem({ onTimeout, id: 'baseline', dismissal: 'transient', timeoutMs: 4000 })
    await tick()

    vi.advanceTimersByTime(3999)
    expect(onTimeout).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('baseline')
  })

  it('keeps the original deadline when the pointer leaves well inside the natural window', async () => {
    const onTimeout = vi.fn()
    const target = mountItem({ onTimeout, id: 'inside', dismissal: 'transient', timeoutMs: 4000 })
    await tick()

    // Hover from t=500 to t=1000: the hover neither pauses nor extends the clock.
    const toast = target.querySelector('.toast') as HTMLElement
    vi.advanceTimersByTime(500)
    toast.dispatchEvent(new PointerEvent('pointerenter'))
    vi.advanceTimersByTime(500)
    toast.dispatchEvent(new PointerEvent('pointerleave'))

    // 3000 ms of the natural window are left, and that beats the 1000 ms tail,
    // so the toast must live the full remainder — not be shortened to the tail.
    vi.advanceTimersByTime(2999)
    expect(onTimeout).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('inside')
  })

  it('holds a toast hovered past its natural deadline, then hides it one second after the pointer leaves', async () => {
    const onTimeout = vi.fn()
    const target = mountItem({ onTimeout, id: 'past', dismissal: 'transient', timeoutMs: 4000 })
    await tick()

    // Hover at t=1000 (well inside the window) and stay until t=20000.
    const toast = target.querySelector('.toast') as HTMLElement
    vi.advanceTimersByTime(1000)
    toast.dispatchEvent(new PointerEvent('pointerenter'))
    vi.advanceTimersByTime(19000)
    expect(onTimeout).not.toHaveBeenCalled()

    // The natural deadline is long gone, so the 1000 ms tail decides.
    toast.dispatchEvent(new PointerEvent('pointerleave'))
    vi.advanceTimersByTime(HOVER_LEAVE_GRACE_MS - 1)
    expect(onTimeout).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('past')
  })

  it('gives the one-second tail when the pointer leaves just before the natural deadline', async () => {
    const onTimeout = vi.fn()
    const target = mountItem({ onTimeout, id: 'tail', dismissal: 'transient', timeoutMs: 4000 })
    await tick()

    // Leave at t=3900: only 100 ms of the natural window remain, so the tail wins.
    const toast = target.querySelector('.toast') as HTMLElement
    vi.advanceTimersByTime(3800)
    toast.dispatchEvent(new PointerEvent('pointerenter'))
    vi.advanceTimersByTime(100)
    toast.dispatchEvent(new PointerEvent('pointerleave'))

    vi.advanceTimersByTime(999)
    expect(onTimeout).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('tail')
  })

  it('persistent toast has no timer regardless of hover', async () => {
    const onTimeout = vi.fn()
    const target = mountItem({ onTimeout, id: 'persistent', dismissal: 'persistent', timeoutMs: 0 })
    await tick()

    const toast = target.querySelector('.toast') as HTMLElement
    toast.dispatchEvent(new PointerEvent('pointerenter'))
    vi.advanceTimersByTime(100000)
    toast.dispatchEvent(new PointerEvent('pointerleave'))
    vi.advanceTimersByTime(100000)

    expect(onTimeout).not.toHaveBeenCalled()
  })
})
