/** Unit tests for the cloud AI consent state (refresh / accept / decline, fail-closed, the held "no"). */

import { describe, it, expect, vi, beforeEach } from 'vitest'

import type { CloudAiConsentStatus } from '$lib/tauri-commands'

const { statusMock, acceptMock, revokeMock, pendingChangedMock, listenMock, settingsMock, order } = vi.hoisted(() => {
  const order: string[] = []
  // An annotation, not an `as`: the lint auto-fix strips an assertion it thinks is unnecessary.
  const values: Record<string, unknown> = {}
  return {
    order,
    statusMock: vi.fn<() => Promise<CloudAiConsentStatus>>(),
    acceptMock: vi.fn<() => Promise<void>>(),
    revokeMock: vi.fn<() => Promise<void>>(),
    pendingChangedMock: vi.fn<() => Promise<void>>(() => Promise.resolve()),
    listenMock: vi.fn<(handler: () => void) => Promise<() => void>>(),
    settingsMock: {
      values,
      forceSave: vi.fn<() => Promise<boolean>>(() => Promise.resolve(true)),
    },
  }
})

vi.mock('$lib/tauri-commands', () => ({
  cloudAiConsentStatus: () => statusMock(),
  acceptCloudAiConsent: () => {
    order.push('accept')
    return acceptMock()
  },
  revokeCloudAiConsent: () => revokeMock(),
  cloudAiConsentRevokePendingChanged: () => pendingChangedMock(),
  onCloudAiConsentChanged: (handler: () => void) => listenMock(handler),
}))
vi.mock('$lib/settings', () => ({
  getSetting: (id: string): unknown => settingsMock.values[id] ?? false,
  setSetting: (id: string, value: unknown) => {
    order.push(`set ${id}=${String(value)}`)
    settingsMock.values[id] = value
  },
  forceSave: () => {
    order.push('save')
    return settingsMock.forceSave()
  },
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import {
  cloudConsentState,
  refreshCloudConsent,
  acceptCloudConsent,
  declineCloudConsent,
  settleHeldCloudConsentRevoke,
  cloudAiBlocked,
  _resetCloudConsentForTests,
} from './cloud-consent.svelte'

const HELD = 'ai.cloudConsentRevokePending'

beforeEach(() => {
  // Reset, not clear: a `mockRejectedValueOnce` a test didn't consume must not leak into the next.
  vi.resetAllMocks()
  order.length = 0
  settingsMock.values = {}
  settingsMock.forceSave.mockResolvedValue(true)
  pendingChangedMock.mockResolvedValue(undefined)
  listenMock.mockResolvedValue(() => undefined)
  _resetCloudConsentForTests()
})

const notAccepted: CloudAiConsentStatus = {
  accepted: false,
  currentVersion: 1,
  acceptedVersion: null,
  acceptedAt: null,
  managed: null,
}
const accepted: CloudAiConsentStatus = {
  accepted: true,
  currentVersion: 1,
  acceptedVersion: 1,
  acceptedAt: 1_760_000_100,
  managed: null,
}

describe('refreshCloudConsent', () => {
  it('applies an accepted status (accepted + timestamp)', async () => {
    statusMock.mockResolvedValue(accepted)
    await refreshCloudConsent()
    expect(cloudConsentState.accepted).toBe(true)
    expect(cloudConsentState.acceptedAt).toBe(1_760_000_100)
  })

  it('clears the timestamp when not accepted', async () => {
    statusMock.mockResolvedValue({ ...notAccepted, acceptedVersion: 1, acceptedAt: 1_760_000_100 })
    await refreshCloudConsent()
    expect(cloudConsentState.accepted).toBe(false)
    expect(cloudConsentState.acceptedAt).toBeNull()
  })

  it('fails CLOSED when the status read throws', async () => {
    statusMock.mockRejectedValue(new Error('nope'))
    await refreshCloudConsent()
    expect(cloudConsentState.accepted).toBe(false)
    expect(cloudConsentState.acceptedAt).toBeNull()
  })

  it('retries a held "no", and lets go of it once the store takes it', async () => {
    settingsMock.values[HELD] = true
    revokeMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue(notAccepted)

    await refreshCloudConsent()

    expect(revokeMock).toHaveBeenCalledOnce()
    expect(settingsMock.values[HELD]).toBe(false)
    expect(settingsMock.forceSave).toHaveBeenCalled()
    expect(pendingChangedMock).toHaveBeenCalled()
  })

  it('keeps holding the "no" when the store refuses the retry too', async () => {
    settingsMock.values[HELD] = true
    revokeMock.mockRejectedValue(new Error('disk I/O error'))
    statusMock.mockResolvedValue(notAccepted)

    await refreshCloudConsent()

    expect(revokeMock).toHaveBeenCalledOnce()
    expect(settingsMock.values[HELD]).toBe(true)
  })

  it('makes no revoke attempt when no "no" is held', async () => {
    statusMock.mockResolvedValue(notAccepted)
    await refreshCloudConsent()
    expect(revokeMock).not.toHaveBeenCalled()
  })

  it('re-reads the status whenever any window announces a consent change', async () => {
    statusMock.mockResolvedValue(notAccepted)
    await refreshCloudConsent()
    expect(listenMock).toHaveBeenCalledOnce()

    statusMock.mockResolvedValue(accepted)
    const handler = listenMock.mock.calls[0][0]
    handler()
    await vi.waitFor(() => {
      expect(cloudConsentState.accepted).toBe(true)
    })

    // One subscription per window, however often the gates refresh.
    await refreshCloudConsent()
    expect(listenMock).toHaveBeenCalledOnce()
  })
})

describe('settleHeldCloudConsentRevoke', () => {
  it('is the same retry the launch runs, and does nothing without a held "no"', async () => {
    await settleHeldCloudConsentRevoke()
    expect(revokeMock).not.toHaveBeenCalled()

    settingsMock.values[HELD] = true
    revokeMock.mockResolvedValue(undefined)
    await settleHeldCloudConsentRevoke()
    expect(revokeMock).toHaveBeenCalledOnce()
    expect(settingsMock.values[HELD]).toBe(false)
  })
})

describe('acceptCloudConsent', () => {
  it('records consent, refreshes, and answers done', async () => {
    acceptMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue(accepted)
    expect(await acceptCloudConsent()).toBe('done')
    expect(acceptMock).toHaveBeenCalledOnce()
    expect(cloudConsentState.accepted).toBe(true)
  })

  it('answers notSaved when the store refuses the write, and the gate stays shut', async () => {
    acceptMock.mockRejectedValue(new Error('database is locked'))
    statusMock.mockResolvedValue(notAccepted)
    expect(await acceptCloudConsent()).toBe('notSaved')
    expect(cloudConsentState.accepted).toBe(false)
  })

  it('answers notSaved when the write went through but the store still reads not accepted', async () => {
    acceptMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue(notAccepted)
    expect(await acceptCloudConsent()).toBe('notSaved')
  })

  it('lets go of a held "no" before recording a deliberate yes', async () => {
    settingsMock.values[HELD] = true
    acceptMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue(accepted)

    await acceptCloudConsent()

    expect(settingsMock.values[HELD]).toBe(false)
    expect(order.indexOf(`set ${HELD}=false`)).toBeLessThan(order.indexOf('accept'))
    expect(order.indexOf('save')).toBeLessThan(order.indexOf('accept'))
    expect(revokeMock).not.toHaveBeenCalled()
  })
})

describe('declineCloudConsent', () => {
  it('turns cloud AI off, with nothing held, when the store takes the first try', async () => {
    revokeMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue(notAccepted)

    expect(await declineCloudConsent()).toBe('done')
    expect(revokeMock).toHaveBeenCalledOnce()
    expect(settingsMock.values[HELD]).toBeUndefined()
    expect(cloudConsentState.accepted).toBe(false)
  })

  it('gives a refused "no" one more try before holding it', async () => {
    revokeMock.mockRejectedValueOnce(new Error('database is locked')).mockResolvedValue(undefined)
    statusMock.mockResolvedValue(notAccepted)

    expect(await declineCloudConsent()).toBe('done')
    expect(revokeMock).toHaveBeenCalledTimes(2)
    expect(settingsMock.values[HELD]).toBeUndefined()
  })

  it('holds a "no" the store refuses twice, saved before the gates are told, and answers done', async () => {
    revokeMock.mockRejectedValue(new Error('disk I/O error'))
    statusMock.mockResolvedValue(notAccepted)

    expect(await declineCloudConsent()).toBe('done')
    expect(settingsMock.values[HELD]).toBe(true)
    // Saved BEFORE the backend is told: the gates read `settings.json` from disk.
    expect(order.slice(0, 2)).toEqual([`set ${HELD}=true`, 'save'])
    expect(pendingChangedMock).toHaveBeenCalled()
  })

  it('answers notSaved only when settings.json won\'t hold the "no" either', async () => {
    revokeMock.mockRejectedValue(new Error('disk I/O error'))
    settingsMock.forceSave.mockResolvedValue(false)
    statusMock.mockResolvedValue(accepted)

    expect(await declineCloudConsent()).toBe('notSaved')
  })
})

describe('cloudAiBlocked', () => {
  it('blocks Cloud until consent reads accepted, and counts "not known yet" as blocked', () => {
    cloudConsentState.accepted = null
    expect(cloudAiBlocked('cloud')).toBe(true)
    cloudConsentState.accepted = false
    expect(cloudAiBlocked('cloud')).toBe(true)
    cloudConsentState.accepted = true
    expect(cloudAiBlocked('cloud')).toBe(false)
  })

  it('never blocks Local or off: nothing leaves the Mac there', () => {
    cloudConsentState.accepted = false
    expect(cloudAiBlocked('local')).toBe(false)
    expect(cloudAiBlocked('off')).toBe(false)
  })
})
