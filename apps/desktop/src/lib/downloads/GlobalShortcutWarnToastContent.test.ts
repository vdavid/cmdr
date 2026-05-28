import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'

const { dismissToastMock, setSettingMock, setGlobalRevealShortcutMock, getSettingMock } = vi.hoisted(() => ({
  dismissToastMock: vi.fn(),
  setSettingMock: vi.fn(),
  setGlobalRevealShortcutMock: vi.fn(),
  getSettingMock: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/settings', () => ({
  setSetting: setSettingMock,
  getSetting: getSettingMock,
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    setGlobalRevealShortcut: setGlobalRevealShortcutMock,
  },
}))

import GlobalShortcutWarnToastContent from './GlobalShortcutWarnToastContent.svelte'

describe('GlobalShortcutWarnToastContent', () => {
  beforeEach(() => {
    dismissToastMock.mockReset()
    setSettingMock.mockReset()
    setGlobalRevealShortcutMock.mockReset().mockResolvedValue({ status: 'ok', data: null })
    getSettingMock.mockReset().mockImplementation((id: string) => {
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.binding') return '⌃⌥⌘J'
      return undefined
    })
  })

  it('renders the binding-aware copy from the snapshotted prop', () => {
    const target = document.createElement('div')
    mount(GlobalShortcutWarnToastContent, {
      target,
      props: { toastId: 'shortcut-warn', binding: '⌃⌥⌘J' },
    })
    flushSync()
    expect(target.textContent).toContain('⌃⌥⌘J')
    expect(target.textContent.toLowerCase()).toContain('keep')
  })

  it('"Keep it on" dismisses the toast (acknowledged is set by the bridge, not the toast)', async () => {
    const target = document.createElement('div')
    mount(GlobalShortcutWarnToastContent, {
      target,
      props: { toastId: 'shortcut-warn', binding: '⌃⌥⌘J' },
    })
    flushSync()
    const keepButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.toLowerCase().includes('keep'),
    )
    if (!keepButton) throw new Error('Keep button not found')
    keepButton.click()
    await tick()

    expect(dismissToastMock).toHaveBeenCalledWith('shortcut-warn')
    // The acknowledged flag is already true at toast creation time (the bridge
    // sets it before opening the toast), so the toast itself doesn't re-write it.
    expect(setSettingMock).not.toHaveBeenCalledWith(
      'behavior.fileSystemWatching.globalRevealShortcut.acknowledged',
      true,
    )
  })

  it('"Turn it off" flips enabled to false AND calls setGlobalRevealShortcut(false, ...)', async () => {
    const target = document.createElement('div')
    mount(GlobalShortcutWarnToastContent, {
      target,
      props: { toastId: 'shortcut-warn', binding: '⌃⌥⌘J' },
    })
    flushSync()
    const offButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.toLowerCase().includes('turn it off'),
    )
    if (!offButton) throw new Error('Turn-it-off button not found')
    offButton.click()
    await tick()
    // Allow microtasks for awaited backend call.
    await new Promise((r) => setTimeout(r, 0))

    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalRevealShortcut.enabled', false)
    expect(setGlobalRevealShortcutMock).toHaveBeenCalledTimes(1)
    const args = setGlobalRevealShortcutMock.mock.calls[0]
    expect(args[0]).toBe(false)
    expect(args[1]).toBe('⌃⌥⌘J')
    expect(dismissToastMock).toHaveBeenCalledWith('shortcut-warn')
  })
})
