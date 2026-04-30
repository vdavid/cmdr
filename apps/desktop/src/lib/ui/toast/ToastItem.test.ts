/**
 * Component-level test for `ToastItem.svelte`.
 *
 * Focuses on the close-button tooltip and the `onUserDismiss` vs `onTimeout`
 * split — both added so the AI download toast can show a tooltip on its X
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
