import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  getFinderExtensionStatus: vi.fn(),
  setFinderExtensionEnabled: vi.fn(),
  listen: vi.fn(),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    getFinderExtensionStatus: mocks.getFinderExtensionStatus,
    setFinderExtensionEnabled: mocks.setFinderExtensionEnabled,
  },
  events: { revealTabsDropped: { listen: mocks.listen } },
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { getFinderExtensionStatus, onRevealTabsDropped, setFinderExtensionEnabled } from './reveal'

describe('the Finder extension wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('passes on what macOS says', async () => {
    const status = { enabled: true, blockedBy: null }
    mocks.getFinderExtensionStatus.mockResolvedValue(status)
    expect(await getFinderExtensionStatus()).toEqual(status)
  })

  it('answers null where the command does not exist', async () => {
    mocks.getFinderExtensionStatus.mockRejectedValue(new Error('no such command'))
    expect(await getFinderExtensionStatus()).toBeNull()
  })

  it('returns the state a switch left, whatever it asked for', async () => {
    const status = { enabled: false, blockedBy: 'notInApplications' }
    mocks.setFinderExtensionEnabled.mockResolvedValue(status)
    expect(await setFinderExtensionEnabled(true)).toEqual(status)
    expect(mocks.setFinderExtensionEnabled).toHaveBeenCalledWith(true)
  })

  it('answers null when the switch can not be reached', async () => {
    mocks.setFinderExtensionEnabled.mockRejectedValue(new Error('no such command'))
    expect(await setFinderExtensionEnabled(true)).toBeNull()
  })
})

describe('onRevealTabsDropped', () => {
  it('hands over just the counts', async () => {
    const stop = vi.fn()
    mocks.listen.mockImplementation((listener: (event: { payload: unknown }) => void) => {
      listener({ payload: { opened: 9, dropped: 3 } })
      return Promise.resolve(stop)
    })
    const handler = vi.fn()
    expect(await onRevealTabsDropped(handler)).toBe(stop)
    expect(handler).toHaveBeenCalledWith({ opened: 9, dropped: 3 })
  })
})
