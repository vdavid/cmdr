/** Unit tests for the consent gate state module (refresh / accept / revoke, fail-closed). */

import { describe, it, expect, vi, beforeEach } from 'vitest'

import type { AskCmdrConsentStatus } from '$lib/tauri-commands'

const { statusMock, acceptMock, revokeMock } = vi.hoisted(() => ({
  statusMock: vi.fn<() => Promise<AskCmdrConsentStatus>>(),
  acceptMock: vi.fn<() => Promise<void>>(),
  revokeMock: vi.fn<() => Promise<void>>(),
}))

vi.mock('$lib/tauri-commands', () => ({
  askCmdrConsentStatus: () => statusMock(),
  acceptAskCmdrConsent: () => acceptMock(),
  revokeAskCmdrConsent: () => revokeMock(),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { consentState, refreshConsent, acceptConsent, revokeConsent } from './ask-cmdr-consent.svelte'

beforeEach(() => {
  vi.clearAllMocks()
  consentState.accepted = null
  consentState.acceptedAt = null
})

describe('refreshConsent', () => {
  it('applies an accepted status (accepted + timestamp)', async () => {
    statusMock.mockResolvedValue({ accepted: true, currentVersion: 1, acceptedVersion: 1, acceptedAt: 1_760_000_000 })
    await refreshConsent()
    expect(consentState.accepted).toBe(true)
    expect(consentState.acceptedAt).toBe(1_760_000_000)
  })

  it('clears the timestamp when not accepted', async () => {
    statusMock.mockResolvedValue({ accepted: false, currentVersion: 1, acceptedVersion: null, acceptedAt: null })
    await refreshConsent()
    expect(consentState.accepted).toBe(false)
    expect(consentState.acceptedAt).toBeNull()
  })

  it('fails CLOSED when the status read throws', async () => {
    statusMock.mockRejectedValue(new Error('nope'))
    await refreshConsent()
    expect(consentState.accepted).toBe(false)
    expect(consentState.acceptedAt).toBeNull()
  })
})

describe('acceptConsent', () => {
  it('records consent, refreshes, and returns the new accepted state', async () => {
    acceptMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue({ accepted: true, currentVersion: 1, acceptedVersion: 1, acceptedAt: 1_760_000_100 })
    const result = await acceptConsent()
    expect(acceptMock).toHaveBeenCalledOnce()
    expect(result).toBe(true)
    expect(consentState.accepted).toBe(true)
  })
})

describe('revokeConsent', () => {
  it('clears consent and refreshes to not-accepted', async () => {
    revokeMock.mockResolvedValue(undefined)
    statusMock.mockResolvedValue({ accepted: false, currentVersion: 1, acceptedVersion: null, acceptedAt: null })
    await revokeConsent()
    expect(revokeMock).toHaveBeenCalledOnce()
    expect(consentState.accepted).toBe(false)
  })
})
