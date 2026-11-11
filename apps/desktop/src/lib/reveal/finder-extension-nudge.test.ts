/** The offer's rule, its raise, and its answers, covered together since they're one flow. */

import { capabilitiesWith } from '#lib/platform/test-platform.js'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FinderExtensionStatus } from '#lib/ipc/bindings.js'
import { emptyNudgeLedger } from '#lib/nudges/nudge-ledger.js'

const mocks = vi.hoisted(() => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
  setSetting: vi.fn(),
  trackEvent: vi.fn(),
  setFinderExtensionEnabled: vi.fn(),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: mocks.addToast, dismissToast: mocks.dismissToast }))
vi.mock('#lib/settings/index.js', () => ({ setSetting: mocks.setSetting, getSetting: vi.fn(() => '') }))
vi.mock('#lib/tauri-commands/index.js', () => ({
  trackEvent: mocks.trackEvent,
  setFinderExtensionEnabled: mocks.setFinderExtensionEnabled,
}))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: (key: string) => key }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { FINDER_EXTENSION_NUDGE_TOAST_ID, offerFinderExtension } from './finder-extension-nudge'
import { acceptFinderExtensionNudge, declineFinderExtensionNudge } from './finder-extension-nudge-answer'
import { FINDER_EXTENSION_NUDGE_AFTER_DAYS, shouldShowFinderExtensionNudge } from './should-show-finder-extension-nudge'

const off: FinderExtensionStatus = { enabled: false, blockedBy: null }

function inputs(overrides: Partial<Parameters<typeof shouldShowFinderExtensionNudge>[0]> = {}) {
  return {
    automatedRun: false,
    capabilities: capabilitiesWith('finderExtension'),
    onboarded: true,
    onboardingShowing: false,
    ledger: emptyNudgeLedger(),
    now: new Date('2026-11-02T12:00:00Z'),
    launchDayCount: FINDER_EXTENSION_NUDGE_AFTER_DAYS,
    extensionStatus: off,
    ...overrides,
  }
}

describe('shouldShowFinderExtensionNudge', () => {
  it('speaks once the launch days are there and the extension is off', () => {
    expect(shouldShowFinderExtensionNudge(inputs())).toBe(true)
  })

  it('waits for the launch days', () => {
    const launchDayCount = FINDER_EXTENSION_NUDGE_AFTER_DAYS - 1
    expect(shouldShowFinderExtensionNudge(inputs({ launchDayCount }))).toBe(false)
  })

  it('stays silent when the extension is already on', () => {
    const extensionStatus = { enabled: true, blockedBy: null }
    expect(shouldShowFinderExtensionNudge(inputs({ extensionStatus }))).toBe(false)
  })

  it('stays silent when the click would be refused, and when nothing answered', () => {
    const blocked: FinderExtensionStatus = { enabled: false, blockedBy: 'notInApplications' }
    expect(shouldShowFinderExtensionNudge(inputs({ extensionStatus: blocked }))).toBe(false)
    expect(shouldShowFinderExtensionNudge(inputs({ extensionStatus: null }))).toBe(false)
  })

  it('asks once ever', () => {
    const ledger = { ...emptyNudgeLedger(), finderExtension: '2026-01-01T00:00:00.000Z' }
    expect(shouldShowFinderExtensionNudge(inputs({ ledger }))).toBe(false)
  })
})

describe('the offer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('stamps the ledger and counts the offer as the toast goes up, and stays until answered', () => {
    offerFinderExtension()
    expect(mocks.setSetting).toHaveBeenCalledWith('behavior.finderExtensionNudgeOfferedAt', expect.any(String))
    expect(mocks.trackEvent).toHaveBeenCalledWith('finder_extension_offered')
    expect(mocks.addToast).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ dismissal: 'persistent', id: FINDER_EXTENSION_NUDGE_TOAST_ID }),
    )
  })

  it('records the × as dismissed', () => {
    offerFinderExtension()
    const options = mocks.addToast.mock.calls[0][1] as { onDismiss: () => void }
    options.onDismiss()
    expect(mocks.trackEvent).toHaveBeenCalledWith('finder_extension_answered', { answer: 'dismissed' })
  })

  it('declining retires the toast and changes nothing', () => {
    declineFinderExtensionNudge('t')
    expect(mocks.dismissToast).toHaveBeenCalledWith('t')
    expect(mocks.trackEvent).toHaveBeenCalledWith('finder_extension_answered', { answer: 'no' })
    expect(mocks.setFinderExtensionEnabled).not.toHaveBeenCalled()
  })

  it('accepting says done only when macOS really switched it on', async () => {
    mocks.setFinderExtensionEnabled.mockResolvedValue({ enabled: true, blockedBy: null })
    await acceptFinderExtensionNudge('t')
    expect(mocks.setFinderExtensionEnabled).toHaveBeenCalledWith(true)
    expect(mocks.trackEvent).toHaveBeenCalledWith('finder_extension_answered', { answer: 'yes' })
    expect(mocks.addToast).toHaveBeenCalledWith('main.finderExtensionNudge.turnedOn', { level: 'success' })
  })

  it.each([
    ['stayedOff', { enabled: false, blockedBy: null }],
    ['notInApplications', { enabled: false, blockedBy: 'notInApplications' }],
    ['unavailable', null],
  ])('accepting reports %s when it did not switch on', async (reason, status) => {
    mocks.setFinderExtensionEnabled.mockResolvedValue(status)
    await acceptFinderExtensionNudge('t')
    expect(mocks.trackEvent).toHaveBeenCalledWith('finder_extension_not_enabled', { reason })
    expect(mocks.addToast).toHaveBeenCalledWith('main.finderExtensionNudge.notTurnedOn', { level: 'warn' })
  })
})
