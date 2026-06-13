/**
 * Behaviour tests for the effectful startup trigger: the IPC fetch + settings writes that
 * sit on top of the pure `decideWhatsNew`. The pure decision table is covered separately in
 * `whats-new.test.ts`; here we pin the side effects (stamp vs open vs collapse-to-stamp,
 * and the no-stamp dev override).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const getVersionMock = vi.fn<() => Promise<string>>(() => Promise.resolve('0.26.0'))
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: () => getVersionMock(),
}))

interface MockRelease {
  version: string
  date: string
  lead: string | null
  sections: { title: string; entries: string[] }[]
}
const getWhatsNewMock = vi.fn<(since: string | null, max: number) => Promise<MockRelease[]>>()
const whatsNewDevOverrideMock = vi.fn<() => Promise<string | null>>(() => Promise.resolve(null))
vi.mock('$lib/tauri-commands', () => ({
  getWhatsNew: (since: string | null, max: number) => getWhatsNewMock(since, max),
  whatsNewDevOverride: () => whatsNewDevOverrideMock(),
}))

let mockLastSeen = ''
let mockEnabled = true
const setSettingMock = vi.fn()
vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((id: string) => {
    if (id === 'whatsNew.lastSeenVersion') return mockLastSeen
    if (id === 'whatsNew.showOnUpdate') return mockEnabled
    return undefined
  }),
  setSetting: (id: string, value: unknown) => {
    setSettingMock(id, value)
  },
}))

const sampleRelease = {
  version: '0.26.0',
  date: '2026-06-11',
  lead: 'A great release.',
  sections: [{ title: 'Added', entries: ['A new thing'] }],
}

import { runWhatsNewStartupTrigger, openWhatsNew, whatsNewState, closeWhatsNew } from './whats-new-trigger.svelte'

const openGates = { onboarded: true, onboardingShowing: false, otherStartupModalOpen: false }

describe('runWhatsNewStartupTrigger', () => {
  beforeEach(() => {
    mockLastSeen = ''
    mockEnabled = true
    getVersionMock.mockResolvedValue('0.26.0')
    getWhatsNewMock.mockReset()
    getWhatsNewMock.mockResolvedValue([sampleRelease])
    whatsNewDevOverrideMock.mockReset()
    whatsNewDevOverrideMock.mockResolvedValue(null)
    setSettingMock.mockClear()
    closeWhatsNew()
    whatsNewState.releases = []
    whatsNewState.allowEmpty = false
  })

  it('opens the dialog and stamps on a real upgrade with a non-empty slice', async () => {
    mockLastSeen = '0.20.0'
    await runWhatsNewStartupTrigger(openGates)

    expect(getWhatsNewMock).toHaveBeenCalledWith('0.20.0', 5)
    expect(whatsNewState.open).toBe(true)
    expect(whatsNewState.releases).toEqual([sampleRelease])
    expect(whatsNewState.allowEmpty).toBe(false)
    expect(setSettingMock).toHaveBeenCalledWith('whatsNew.lastSeenVersion', '0.26.0')
  })

  it('collapses an empty slice on a show decision to a silent stamp (no popup)', async () => {
    mockLastSeen = '0.20.0'
    getWhatsNewMock.mockResolvedValue([])
    await runWhatsNewStartupTrigger(openGates)

    expect(whatsNewState.open).toBe(false)
    expect(setSettingMock).toHaveBeenCalledWith('whatsNew.lastSeenVersion', '0.26.0')
  })

  it('stamps silently on a fresh install without fetching or opening', async () => {
    mockLastSeen = ''
    await runWhatsNewStartupTrigger({ ...openGates, onboarded: false })

    expect(getWhatsNewMock).not.toHaveBeenCalled()
    expect(whatsNewState.open).toBe(false)
    expect(setSettingMock).toHaveBeenCalledWith('whatsNew.lastSeenVersion', '0.26.0')
  })

  it('waits (no stamp, no open) when a startup modal blocks a would-show', async () => {
    mockLastSeen = '0.20.0'
    await runWhatsNewStartupTrigger({ ...openGates, otherStartupModalOpen: true })

    expect(whatsNewState.open).toBe(false)
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('does not stamp when the changelog fetch throws on a show decision', async () => {
    mockLastSeen = '0.20.0'
    getWhatsNewMock.mockRejectedValue(new Error('ipc down'))
    await runWhatsNewStartupTrigger(openGates)

    expect(whatsNewState.open).toBe(false)
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('dev override forces the show path from the given version and never stamps', async () => {
    // Setting is off and onboarding is up: the override must bypass both.
    mockEnabled = false
    whatsNewDevOverrideMock.mockResolvedValue('0.22.0')
    await runWhatsNewStartupTrigger({ onboarded: true, onboardingShowing: true, otherStartupModalOpen: true })

    expect(getWhatsNewMock).toHaveBeenCalledWith('0.22.0', 5)
    expect(whatsNewState.open).toBe(true)
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})

describe('openWhatsNew (manual M3 seam)', () => {
  beforeEach(() => {
    getWhatsNewMock.mockReset()
    getWhatsNewMock.mockResolvedValue([sampleRelease])
    setSettingMock.mockClear()
    closeWhatsNew()
  })

  it('fetches the latest five with no lower bound, opens, allows empty, and never stamps', async () => {
    await openWhatsNew()

    expect(getWhatsNewMock).toHaveBeenCalledWith(null, 5)
    expect(whatsNewState.open).toBe(true)
    expect(whatsNewState.allowEmpty).toBe(true)
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})
