/**
 * The two halves of the main window's Escape: a stopped Escape counts as used (so
 * AppKit never sees it and leaves full screen), and an unused one leaves full
 * screen only when the setting says so, telling the user once.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { withCapabilities } from '#lib/platform/test-platform.js'

const windowMock = vi.hoisted(() => ({
  isFullscreen: vi.fn(() => Promise.resolve(true)),
  setFullscreen: vi.fn(() => Promise.resolve()),
}))
const settingValues = vi.hoisted(() => new Map<string, unknown>())
const addToast = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => windowMock }))
vi.mock('#lib/settings/index.js', () => ({
  getSetting: (id: string) => settingValues.get(id),
  setSetting: (id: string, value: unknown) => {
    settingValues.set(id, value)
  },
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))
vi.mock('./EscapeFullScreenToastContent.svelte', () => ({ default: {} }))

import { exitFullScreenOnEscape, installEscapeStopClaims } from './escape-key'

function escape(): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
}

describe('installEscapeStopClaims', () => {
  let uninstall: () => void
  let target: HTMLElement

  beforeEach(() => {
    uninstall = installEscapeStopClaims()
    target = document.createElement('div')
    document.body.appendChild(target)
  })

  afterEach(() => {
    uninstall()
    target.remove()
  })

  it('prevents the default of an Escape a handler stopped', () => {
    target.addEventListener('keydown', (e) => {
      e.stopPropagation()
    })
    const event = escape()
    target.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(true)
  })

  it('covers stopImmediatePropagation too', () => {
    target.addEventListener('keydown', (e) => {
      e.stopImmediatePropagation()
    })
    const event = escape()
    target.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(true)
  })

  it('leaves an Escape nobody stopped, and other keys, alone', () => {
    const event = escape()
    target.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(false)

    target.addEventListener('keydown', (e) => {
      e.stopPropagation()
    })
    const enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
    target.dispatchEvent(enter)
    expect(enter.defaultPrevented).toBe(false)
  })
})

describe('exitFullScreenOnEscape', () => {
  beforeEach(() => {
    settingValues.clear()
    settingValues.set('advanced.exitFullScreenOnEscape', true)
    settingValues.set('advanced.exitFullScreenOnEscapeHintShown', false)
    windowMock.isFullscreen.mockResolvedValue(true)
    windowMock.setFullscreen.mockClear()
    windowMock.isFullscreen.mockClear()
    addToast.mockClear()
    return withCapabilities('fullScreen')
  })

  // Where Cmdr offers no way into full screen, the setting and its hint are hidden too.
  it('does nothing where the OS has no full screen of Cmdr’s', async () => {
    withCapabilities()
    await exitFullScreenOnEscape()
    expect(windowMock.isFullscreen).not.toHaveBeenCalled()
    expect(windowMock.setFullscreen).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('leaves full screen and shows the hint the first time only', async () => {
    await exitFullScreenOnEscape()
    expect(windowMock.setFullscreen).toHaveBeenCalledWith(false)
    expect(addToast).toHaveBeenCalledOnce()

    await exitFullScreenOnEscape()
    expect(windowMock.setFullscreen).toHaveBeenCalledTimes(2)
    expect(addToast).toHaveBeenCalledOnce()
  })

  it('does nothing when the setting is off', async () => {
    settingValues.set('advanced.exitFullScreenOnEscape', false)
    await exitFullScreenOnEscape()
    expect(windowMock.setFullscreen).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('does nothing when the window is not in full screen', async () => {
    windowMock.isFullscreen.mockResolvedValue(false)
    await exitFullScreenOnEscape()
    expect(windowMock.setFullscreen).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })
})
