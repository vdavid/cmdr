import { describe, it, expect, vi, beforeEach } from 'vitest'

const { setSettingMock, getSettingMock } = vi.hoisted(() => ({
  setSettingMock: vi.fn(),
  getSettingMock: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  setSetting: setSettingMock,
  getSetting: getSettingMock,
}))

import { setGlobalGoToLatestBinding } from './global-shortcut-setting'

describe('setGlobalGoToLatestBinding', () => {
  beforeEach(() => {
    setSettingMock.mockReset()
    getSettingMock.mockReset()
  })

  it('writes the new binding AND resets acknowledged to false', () => {
    setGlobalGoToLatestBinding('⌘⇧K')
    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalGoToLatestShortcut.binding', '⌘⇧K')
    expect(setSettingMock).toHaveBeenCalledWith(
      'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged',
      false,
    )
  })

  it('writes both calls in the right order (binding first, ack reset second)', () => {
    setGlobalGoToLatestBinding('⌥⌘P')
    const calls = setSettingMock.mock.calls.map((c) => c[0] as string)
    const bindingIdx = calls.indexOf('behavior.fileSystemWatching.globalGoToLatestShortcut.binding')
    const ackIdx = calls.indexOf('behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged')
    expect(bindingIdx).toBeGreaterThanOrEqual(0)
    expect(ackIdx).toBeGreaterThan(bindingIdx)
  })
})
