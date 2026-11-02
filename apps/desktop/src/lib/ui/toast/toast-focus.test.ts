/**
 * Reaching the toasts from the keyboard: "Move focus to the notifications" lands on the newest
 * toast's first action, the arrows and Tab cycle every toast's buttons (close button first),
 * Enter and Space stay with the focused button, and Escape goes back where focus came from.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import ToastContainer from './ToastContainer.svelte'
import ToastIdFixture from './toast-id-fixture.svelte'
import { addToast, clearAllToasts, getToasts } from './toast-store.svelte'
import { focusNewestToast, toastStackOwnsKey } from './toast-focus'
import { eventually } from '#lib/test-frames.js'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  clearAllToasts()
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  clearAllToasts()
  document.body.innerHTML = ''
})

async function setup(): Promise<{ target: HTMLElement; origin: HTMLButtonElement }> {
  const origin = document.createElement('button')
  origin.textContent = 'Files'
  document.body.appendChild(origin)
  origin.focus()
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(ToastContainer, { target })
  await tick()
  return { target, origin }
}

/**
 * What has focus, the way a person would name it: a close button as `× <its toast's text>`,
 * any other control by its text.
 */
function focused(): string {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return ''
  if (active.classList.contains('toast-close')) {
    return `× ${active.closest('[data-toast-frame]')?.getAttribute('aria-label') ?? ''}`
  }
  return active.getAttribute('aria-label') ?? active.textContent.trim()
}

function press(key: string, { shift = false }: { shift?: boolean } = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, shiftKey: shift, bubbles: true, cancelable: true })
  document.activeElement?.dispatchEvent(event)
  return event
}

async function settle(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

/** An older plain toast under a newer one with an action of its own. */
async function twoToasts(): Promise<void> {
  addToast('Copied 3 files.', { dismissal: 'persistent' })
  addToast(ToastIdFixture, { id: 'newest', dismissal: 'persistent' })
  await tick()
}

describe('Move focus to the notifications', () => {
  it("lands on the newest toast's first action, not its close button", async () => {
    await setup()
    await twoToasts()

    expect(focusNewestToast()).toBe(true)

    expect(focused()).toBe('Dismiss notification')
    expect(document.activeElement?.classList.contains('toast-close')).toBe(false)
  })

  it("lands on an older toast's action when the newest one has only its close button", async () => {
    await setup()
    addToast(ToastIdFixture, { id: 'older', dismissal: 'persistent' })
    addToast('Moved 2 files to the trash.', { dismissal: 'persistent' })
    await tick()

    focusNewestToast()

    expect(focused()).toBe('Dismiss notification')
    expect(document.activeElement?.closest('[data-toast-frame]')?.textContent).toContain('older')
  })

  it("lands on the newest toast's close button when no toast has anything else", async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    addToast('Moved 2 files to the trash.', { dismissal: 'persistent' })
    await tick()

    focusNewestToast()

    expect(focused()).toBe('× Moved 2 files to the trash.')
  })

  it('never focuses a toast frame', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()

    focusNewestToast()
    expect(document.activeElement?.getAttribute('role')).not.toBe('group')
    press('ArrowDown')

    expect(document.activeElement?.getAttribute('role')).not.toBe('group')
    expect(document.querySelector('[data-toast-frame][tabindex]')).toBeNull()
  })

  it('says there is nothing to reach when no toast is up, and leaves focus alone', async () => {
    const { target, origin } = await setup()

    expect(focusNewestToast()).toBe(false)

    await eventually(() => {
      expect(target.querySelector('[data-toast-announcer="polite"]')?.textContent).toBe('No notifications')
    })
    expect(document.activeElement).toBe(origin)
  })

  it('holds a transient toast while focus is on it', async () => {
    vi.useFakeTimers()
    try {
      await setup()
      addToast('Copied 3 files.', { timeoutMs: 4000 })
      await tick()

      focusNewestToast()
      vi.advanceTimersByTime(60_000)

      expect(getToasts()).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('walking the toasts', () => {
  it('↓ cycles every toast, newest first and its close button first, and wraps around', async () => {
    await setup()
    await twoToasts()
    focusNewestToast()

    const walk: string[] = [focused()]
    for (let i = 0; i < 3; i++) {
      press('ArrowDown')
      walk.push(focused())
    }
    expect(walk).toEqual([
      'Dismiss notification',
      '× Copied 3 files.',
      expect.stringMatching(/^× /),
      'Dismiss notification',
    ])
  })

  it('↑ walks back, wrapping from the first stop to the last', async () => {
    await setup()
    await twoToasts()
    focusNewestToast()

    press('ArrowUp')
    expect(focused()).toMatch(/^× /)
    expect(focused()).not.toBe('× Copied 3 files.')

    press('ArrowUp')
    expect(focused()).toBe('× Copied 3 files.')
  })

  it('Tab walks like ↓ and Shift+Tab like ↑, and both stay in the toasts', async () => {
    await setup()
    await twoToasts()
    focusNewestToast()

    const tab = press('Tab')
    expect(tab.defaultPrevented).toBe(true)
    expect(focused()).toBe('× Copied 3 files.')

    press('Tab')
    expect(focused()).toMatch(/^× /)
    expect(focused()).not.toBe('× Copied 3 files.')

    const shiftTab = press('Tab', { shift: true })
    expect(shiftTab.defaultPrevented).toBe(true)
    expect(focused()).toBe('× Copied 3 files.')
  })

  it('claims the arrow keys it acts on, so no pane command runs behind them', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    expect(press('ArrowDown').defaultPrevented).toBe(true)
  })

  it('keeps Enter, Space, and Tab from the command dispatcher', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    expect(toastStackOwnsKey('Enter')).toBe(true)
    expect(toastStackOwnsKey('Space')).toBe(true)
    expect(toastStackOwnsKey('Tab')).toBe(true)
    expect(toastStackOwnsKey('⇧Tab')).toBe(true)
    expect(toastStackOwnsKey('⌘C')).toBe(false)
  })

  it('owns no key while focus is outside the stack', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()

    expect(toastStackOwnsKey('Enter')).toBe(false)
    expect(toastStackOwnsKey('Tab')).toBe(false)
  })
})

describe('leaving the toasts', () => {
  it('Escape goes back where focus came from', async () => {
    const { origin } = await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    const escape = press('Escape')

    expect(escape.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(origin)
  })

  it('closing the focused toast moves focus into the toasts left', async () => {
    await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    addToast('Moved 2 files to the trash.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    ;(document.activeElement as HTMLButtonElement).click()
    await settle()

    expect(focused()).toBe('× Copied 3 files.')
  })

  it('closing the last focused toast goes back where focus came from', async () => {
    const { origin } = await setup()
    addToast('Copied 3 files.', { dismissal: 'persistent' })
    await tick()
    focusNewestToast()

    ;(document.activeElement as HTMLButtonElement).click()
    await settle()

    expect(document.activeElement).toBe(origin)
  })
})
