import { describe, it, expect, afterEach, vi } from 'vitest'
import { trapFocus, _resetForTests } from './focus-trap'

/**
 * The action is a plain function over DOM nodes, so these tests build containers by
 * hand instead of mounting Svelte components. Keyboard events are dispatched manually
 * because happy-dom doesn't simulate browser tabbing; the assertions check what the
 * trap prevented and where it moved focus.
 */

function buildDialog(buttonCount: number): { container: HTMLElement; buttons: HTMLButtonElement[] } {
  const container = document.createElement('div')
  container.tabIndex = -1
  const buttons = Array.from({ length: buttonCount }, (_, i) => {
    const button = document.createElement('button')
    button.textContent = `Button ${String(i)}`
    container.appendChild(button)
    return button
  })
  document.body.appendChild(container)
  return { container, buttons }
}

function pressTab(target: HTMLElement, shiftKey = false): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: 'Tab', shiftKey, bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event
}

function pressEscape(target: HTMLElement): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event
}

/** Focus + an explicit focusin dispatch, so the test doesn't depend on happy-dom's event fidelity. */
function focusWithEvent(el: HTMLElement): void {
  el.focus()
  el.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
}

/** The leak guard defers its pull-back by one microtask; this flushes it. */
async function flushMicrotasks(): Promise<void> {
  await Promise.resolve()
}

afterEach(() => {
  _resetForTests()
  document.body.innerHTML = ''
})

describe('trapFocus: Tab wrapping', () => {
  it('wraps Tab on the last tabbable to the first', () => {
    const { container, buttons } = buildDialog(3)
    trapFocus(container)
    buttons[2].focus()

    const event = pressTab(buttons[2])

    expect(event.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(buttons[0])
  })

  it('wraps Shift+Tab on the first tabbable to the last', () => {
    const { container, buttons } = buildDialog(3)
    trapFocus(container)
    buttons[0].focus()

    const event = pressTab(buttons[0], true)

    expect(event.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(buttons[2])
  })

  it('leaves mid-list Tab to the browser', () => {
    const { container, buttons } = buildDialog(3)
    trapFocus(container)
    buttons[1].focus()

    const event = pressTab(buttons[1])

    expect(event.defaultPrevented).toBe(false)
  })

  it('sends Tab from the container itself to the first tabbable', () => {
    // ModalDialog focuses its overlay (tabindex="-1") on mount; the first Tab must land
    // on the first control, not leak out.
    const { container, buttons } = buildDialog(2)
    trapFocus(container)
    container.focus()

    const event = pressTab(container)

    expect(event.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(buttons[0])
  })

  it('swallows Tab when the container has no tabbables', () => {
    const { container } = buildDialog(0)
    trapFocus(container)
    container.focus()

    const event = pressTab(container)

    expect(event.defaultPrevented).toBe(true)
  })

  it('queries tabbables fresh: a control added after mount joins the cycle', () => {
    const { container, buttons } = buildDialog(1)
    trapFocus(container)
    const added = document.createElement('button')
    container.appendChild(added)
    buttons[0].focus()

    pressTab(added) // From the (new) last element...
    added.focus()
    const event = pressTab(added)

    expect(event.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(buttons[0])
  })

  it('pulls leaked focus back when Tab fires outside the container', () => {
    const { container, buttons } = buildDialog(2)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    trapFocus(container)
    outside.focus()

    const event = pressTab(outside)

    expect(event.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(buttons[0])
  })
})

describe('trapFocus: Escape fallback', () => {
  it('calls onEscape when Escape fires with focus outside the container', () => {
    const { container } = buildDialog(2)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    const onEscape = vi.fn()
    trapFocus(container, { onEscape })
    outside.focus()

    const event = pressEscape(outside)

    expect(onEscape).toHaveBeenCalledOnce()
    expect(event.defaultPrevented).toBe(true)
  })

  it('stays out of the way when Escape fires with focus inside', () => {
    // The dialog's own Escape handler owns the healthy path.
    const { container, buttons } = buildDialog(2)
    const onEscape = vi.fn()
    trapFocus(container, { onEscape })
    buttons[0].focus()

    const event = pressEscape(buttons[0])

    expect(onEscape).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(false)
  })

  it('does nothing on outside Escape when onEscape is not provided', () => {
    // The onboarding wizard intentionally has no Escape path.
    const { container } = buildDialog(1)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    trapFocus(container)
    outside.focus()

    const event = pressEscape(outside)

    expect(event.defaultPrevented).toBe(false)
  })
})

describe('trapFocus: leak guard', () => {
  it('pulls focus back to the last-focused inside element', async () => {
    const { container, buttons } = buildDialog(3)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    trapFocus(container)
    focusWithEvent(buttons[1])

    focusWithEvent(outside)
    await flushMicrotasks()

    expect(document.activeElement).toBe(buttons[1])
  })

  it('falls back to the first tabbable when nothing inside was focused yet', async () => {
    const { container, buttons } = buildDialog(2)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    trapFocus(container)

    focusWithEvent(outside)
    await flushMicrotasks()

    expect(document.activeElement).toBe(buttons[0])
  })

  it('does not yank focus after the trap is destroyed mid-teardown', async () => {
    // Mirrors dialog close: onDestroy restores pane focus, then the action's destroy
    // unregisters the trap before the guard's microtask runs.
    const { container } = buildDialog(2)
    const pane = document.createElement('button')
    document.body.appendChild(pane)
    const action = trapFocus(container)

    focusWithEvent(pane)
    action.destroy?.()
    await flushMicrotasks()

    expect(document.activeElement).toBe(pane)
  })
})

describe('trapFocus: stacking', () => {
  it('only the topmost trap enforces; closing it hands back to the one below', async () => {
    const { container: dialog, buttons: dialogButtons } = buildDialog(2)
    const { container: popover, buttons: popoverButtons } = buildDialog(2)
    trapFocus(dialog)
    const popoverTrap = trapFocus(popover)

    // While the popover is on top, a leak lands back in the popover, not the dialog.
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    focusWithEvent(outside)
    await flushMicrotasks()
    expect(document.activeElement).toBe(popoverButtons[0])

    // Tab wrapping is also scoped to the popover.
    popoverButtons[1].focus()
    pressTab(popoverButtons[1])
    expect(document.activeElement).toBe(popoverButtons[0])

    // After the popover closes, the dialog's trap takes over.
    popoverTrap.destroy?.()
    focusWithEvent(outside)
    await flushMicrotasks()
    expect(document.activeElement).toBe(dialogButtons[0])
  })

  it('routes the Escape fallback to the topmost trap only', () => {
    const { container: dialog } = buildDialog(1)
    const { container: popover } = buildDialog(1)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    const closeDialog = vi.fn()
    const closePopover = vi.fn()
    trapFocus(dialog, { onEscape: closeDialog })
    trapFocus(popover, { onEscape: closePopover })
    outside.focus()

    pressEscape(outside)

    expect(closePopover).toHaveBeenCalledOnce()
    expect(closeDialog).not.toHaveBeenCalled()
  })

  it('update() swaps the options in place', () => {
    const { container } = buildDialog(1)
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    const first = vi.fn()
    const second = vi.fn()
    const action = trapFocus(container, { onEscape: first })
    action.update?.({ onEscape: second })
    outside.focus()

    pressEscape(outside)

    expect(first).not.toHaveBeenCalled()
    expect(second).toHaveBeenCalledOnce()
  })
})
