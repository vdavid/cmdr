import { describe, it, expect, vi, beforeEach } from 'vitest'

const { setSettingMock, getSettingMock } = vi.hoisted(() => ({
  setSettingMock: vi.fn(),
  getSettingMock: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  setSetting: setSettingMock,
  getSetting: getSettingMock,
}))

import { setGlobalRevealBinding } from './global-shortcut-setting'

describe('setGlobalRevealBinding', () => {
  beforeEach(() => {
    setSettingMock.mockReset()
    getSettingMock.mockReset()
  })

  it('writes the new binding AND resets acknowledged to false', () => {
    setGlobalRevealBinding('⌘⇧K')
    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalRevealShortcut.binding', '⌘⇧K')
    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalRevealShortcut.acknowledged', false)
  })

  it('writes both calls in the right order (binding first, ack reset second)', () => {
    setGlobalRevealBinding('⌥⌘P')
    const calls = setSettingMock.mock.calls.map((c) => c[0] as string)
    const bindingIdx = calls.indexOf('behavior.fileSystemWatching.globalRevealShortcut.binding')
    const ackIdx = calls.indexOf('behavior.fileSystemWatching.globalRevealShortcut.acknowledged')
    expect(bindingIdx).toBeGreaterThanOrEqual(0)
    expect(ackIdx).toBeGreaterThan(bindingIdx)
  })
})
