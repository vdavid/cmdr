/**
 * The one chokepoint that starts mDNS, and the two things it must get right.
 *
 * ❗ This is what fires the macOS "Cmdr wants to find devices on local networks"
 * prompt: the OS gates it on the actual multicast browse, not on app startup. So
 * a call that slips past the `network.enabled` gate shows a system permission
 * dialog to someone who turned discovery off, which is the failure this file
 * exists to prevent.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const ensureNetworkDiscoveryStarted = vi.fn(() => Promise.resolve())
const settings = new Map<string, unknown>()

vi.mock('$lib/tauri-commands', () => ({
  ensureNetworkDiscoveryStarted: () => ensureNetworkDiscoveryStarted(),
}))
vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => settings.get(id),
  setSetting: (id: string, value: unknown) => {
    settings.set(id, value)
  },
}))

import { triggerNetworkDiscovery } from './lazy-trigger'

beforeEach(() => {
  vi.clearAllMocks()
  settings.clear()
  settings.set('network.enabled', true)
  settings.set('network.firstTriggerDone', false)
})

describe('triggerNetworkDiscovery', () => {
  it('starts discovery and records that the prompt has been paid for', () => {
    triggerNetworkDiscovery()
    expect(ensureNetworkDiscoveryStarted).toHaveBeenCalledTimes(1)
    // Later launches start mDNS eagerly, so a returning user gets full speed
    // without re-prompting.
    expect(settings.get('network.firstTriggerDone')).toBe(true)
  })

  it('❌ never browses while discovery is off', () => {
    settings.set('network.enabled', false)
    triggerNetworkDiscovery()
    expect(ensureNetworkDiscoveryStarted).not.toHaveBeenCalled()
    expect(settings.get('network.firstTriggerDone')).toBe(false)
  })

  it('is idempotent: a second call re-arms the daemon and writes nothing', () => {
    triggerNetworkDiscovery()
    triggerNetworkDiscovery()
    // The backend command is idempotent, so calling it again is free; the
    // setting is already true, so nothing writes.
    expect(ensureNetworkDiscoveryStarted).toHaveBeenCalledTimes(2)
    expect(settings.get('network.firstTriggerDone')).toBe(true)
  })
})
