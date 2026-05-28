/**
 * Component-level test for `ToastItem.svelte`.
 *
 * Focuses on the close-button tooltip and the `onUserDismiss` vs `onTimeout`
 * split, both added so the AI download toast can show a tooltip on its X
 * button and remember user-driven dismissal.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import ToastItem from './ToastItem.svelte'

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

describe('ToastItem hover-pause and grace timer', () => {
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

  it('hover before expiry pauses the timer; leaving resumes with remaining time', async () => {
    const onTimeout = vi.fn()
    const target = mountItem({ onTimeout, id: 'pause', dismissal: 'transient', timeoutMs: 4000 })
    await tick()

    // 1 second in, hover -> pause.
    vi.advanceTimersByTime(1000)
    const toast = target.querySelector('.toast') as HTMLElement
    toast.dispatchEvent(new PointerEvent('pointerenter'))

    // Spend 10 seconds hovered; no timeout should fire.
    vi.advanceTimersByTime(10000)
    expect(onTimeout).not.toHaveBeenCalled()

    // Leave -> should re-arm with remaining 3000 ms.
    toast.dispatchEvent(new PointerEvent('pointerleave'))
    vi.advanceTimersByTime(2999)
    expect(onTimeout).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('pause')
  })

  it('hover past natural expiry holds the toast; leaving triggers a 2000 ms grace timer', async () => {
    const onTimeout = vi.fn()
    const target = mountItem({ onTimeout, id: 'grace', dismissal: 'transient', timeoutMs: 4000 })
    await tick()

    // Hover immediately, then go past natural expiry.
    const toast = target.querySelector('.toast') as HTMLElement
    toast.dispatchEvent(new PointerEvent('pointerenter'))
    vi.advanceTimersByTime(10000)
    expect(onTimeout).not.toHaveBeenCalled()

    // Leave -> grace timer (2000 ms), not immediate dismissal.
    toast.dispatchEvent(new PointerEvent('pointerleave'))
    vi.advanceTimersByTime(1999)
    expect(onTimeout).not.toHaveBeenCalled()

    vi.advanceTimersByTime(2)
    expect(onTimeout).toHaveBeenCalledWith('grace')
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
